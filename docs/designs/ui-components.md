# vt-tui: components and parameter widgets

A read-only design study, 2026-11-14, against `main` at 1cc0ff7d (after #715, #717 and #734). I
built, ran and edited nothing. Paths are relative to `crates/vt-tui/src/` unless they start with
`vt-model/` (meaning `crates/vt-model/src/`), `tests/` or `examples/` (under `crates/vt-tui/`).

Each prior-art claim carries one of two marks:
- **[read]** means I read it here: in this checkout, in `../dsp` (tracker-ce, picoTracker,
  DelugeFirmware) or in the cargo registry's ratatui 0.30.2 and ratatui-core 0.1.2.
- **[known]** means it comes from what I know of the project. I didn't read it here.

The parallel device-description study (`device-interface.md`) came in near the end. This
study builds on its `ParamView` (§2.5 there) and its PR names (M1, S1, M2, S2, E6/V3/G3), and §3.1
lists four gaps in that contract. It does not redesign the description.

**As built** (2026-11-15). The plan in §8 is built but for U13. The study below is kept as it was
written, so its "today" is `main` before the plan; the design's
[Architecture](vimtracker.md#architecture) describes the code as it is.

| Step | Built in |
|---|---|
| The services split (#320) | #742, #743 |
| U1 popups | #748 |
| U2 widget pins | #747 |
| U3 `CellLook` | #751 |
| U4 `ParamCell` and `TrackCells` | #756 |
| U5 pictures as data (#728) | #753 |
| U6 `TypedValue` | #754 |
| M2 (#726) | #761 |
| U7 `view` and `set` | #772 |
| U8 `param::edit` and `choice_picker` | #782 |
| U9 `Page` and `Page::KEYS` | #759 |
| U10a project and audio pages | #769 |
| U10b sample page | #773 |
| U10c song view | #776 |
| U10d chain view | #779 |
| U10e phrase view | #781 |
| E6/V3/G3, the Roto lookups (#731) | #778 |
| The answers to §1.5 and §9.3: faders with macros, the macro mark, `gd` to a macro | #764, #767, #768 |
| #719's vt-tui part, one test binary | #785 |

Where it differs from the plan:
- #719's vt-tui part landed after the pages, not before the split.
- U10 was five PRs, a to e.
- `Ctx` read through `&App` until the third architecture review's item 1 (#784) made it the
  services, the modes and a `PageState`, in #787.
- The Roto's knob feel kept a match on each `Control` until #784's item 3 read it from the view,
  in #788.
- Pages still pass keys back to `App` only to reach the picker, the views and `:` lines (#784's
  item 2).

What's left: U13, the module layout (#733), and #784's other items: 2, 4 and 9 to 13.

---

## 0. The recommendation on one page

1. **Finish the shape #320 started; don't replace it.**
   - Pages stay structs that own their state, their keys and their drawing, as `TrackPage` and
     `MixerPage` already do.
   - After the services split, `App` is `Services`, plus the pages, plus the modes and overlays
     that span pages.
   - Add one small `Page` trait. It has eight users and is dispatched statically by `View`.
   - No message bus, no `dyn` compositor, no Elm-style `Msg` layer. `Cmd` is already the message
     in and `Action` the message out.
2. **Widgets get plain values, never `Ctx`.**
   - They are immediate-mode ratatui `Widget`s, built each frame, as `ValueBar`, `SettingsGrid`,
     `Device` and `GraphicPanel` already are.
   - #715 made `widgets::setting` take `&Ctx`. Give it a small `CellLook` instead, so every widget
     can be unit-tested against a `Buffer`.
3. **One parameter widget family, driven by the device study's `ParamView`** (its §2.5).
   - `ParamCell` draws. `param::edit` holds pure functions for `= - + _ x K i` and the fine digits.
     `TypedValue` handles Insert's text.
   - Together they replace six `CellSource` bodies, five `title_controls` copies, three live-value
     copies, three copies of the two-hex-digit commit and three typing buffers.
   - Most of the 42 `open_*_picker` functions become one choice picker.
4. **The grid container already exists; complete it.**
   - The chain is `Grid` (vt-model), then `DeviceSpec`, then `SettingsGrid`.
   - Add `DeviceSpec::from_groups`. N parameters become one device per group of up to twelve, as
     4-op's operators already are.
   - Give `CellSource` one `cell()` method.
   - The Roto's pages stay hand-written (#419). Only their key lookups go through the description
     (#731).
5. **Images: a page says which pictures it wants, as data.**
   - `Graphics` becomes a service: the protocol, ids, the panel cache and placements.
   - A page lists `PanelWant`s before drawing and gets back `PanelIds`. That replaces seven
     separate id fields and the 18 reads of `&App` in `Graphics::sync`. This is #728.
6. **Keys: the binding table stays central and static.**
   - Each page's rows move next to the page (`Page::KEYS`). which-key and `Space k` stay generated
     from the table, as they are now.
7. **What it costs.**
   - About a dozen PRs, each S or M and each behaviour-identical, all after the services split.
   - The one real risk is render parity of `ParamCell` in states the screenshots don't cover. Widget
     tests go first, and a temporary parity test compares old and new cell output for every control.

---

## 1. The UI as it is

### 1.1 Pages (`View`, `input/mode.rs:5`)

`run::draw_screen` (`run.rs:580`) draws the page through `match app.view` (`run.rs:602-615`).
`App::run` (`app/keys.rs:468`) dispatches its keys by view. A key no page takes falls to
`run_tree` (`tree.rs:399`), then `run_grammar` (`grammar.rs:407`), then the global
`match cmd` (`app/keys.rs:482-661`).

| View | Drawn | Keys | State | Component yet? |
|---|---|---|---|---|
| Track (Sound, Mod, FX, Gen) | `TrackPage::draw`, `views/track.rs:721`, with `views/track/{sound,modulation,generators,zoom}.rs` | `App::run_track_page` `track_page.rs:605`, then `TrackPage::run` :392 and `TrackPage::key` :353. The edits are `impl App`: `track_page/{editing,pickers,typing,tuning}.rs` | `App.track_page: TrackPage` (`app.rs:262`, struct `track_page.rs:149`): cursor, fine, step, kept places, zoom, 7 image-id fields, meters | Yes: state, own keys and drawing (#715). Edits no |
| Mixer | `MixerPage::draw`, `views/mixer.rs:303` | `App::run_mixer` `mixer_page.rs:134`, then `MixerPage::run`; edits go to `run_track_page` | `App.mixer_page` (`app.rs:249`): one `row` | Yes (#717). `h`/`l`/`0`/`$` stay in `App`, because they move the program's cursor |
| Project (Song) | `views/project.rs:283`, `draw(&App)` | `run_project_page` `project_page.rs:352` | loose on `App`: `project_row/field/fine/wanted` `app.rs:293-296`, `groove_shown` :298 | No |
| Audio | `views/audio.rs:81` | `run_audio_page` `audio_page.rs:264` | `App.audio`, `audio_request`, `audio_row` `app.rs:232-236` | No |
| Sample | `views/sample.rs:250` | `run_sample_page` `sample_page.rs:663`, plus `placing.rs`, `sample_settings.rs`, `slicing.rs`, `marking.rs` | `App.sample_page: SamplePage` (`app.rs:266`, struct `sample_page.rs:109`), `sample_images` :267 | Half: state yes, keys `impl App` |
| Song | `views/song.rs:58` | `run_song` `song_view.rs:64` | `song_column` `app.rs:250`, `section_anchor` :258 | No |
| Chain | `views/chain.rs:18` | `run_chain` `tree.rs:546` | `chain_column` `app.rs:302` | No |
| Phrase | `views/phrase.rs:817` (a 630-line `draw`) | the grammar (`grammar.rs:310`, `:407`) | `App.cursor` `app.rs:310` (`app/cursor.rs`), `top`/`view_height` :392-393 | No |

### 1.2 Overlays, popups and modes

`draw_screen` draws them last, at `run.rs:633-657`. Some are drawn because an `Option` field on
`App` is set (picker, guide, shape, mapping). Others are drawn by `Mode` (Help, the command menu).
`App::handle_key` routes keys by `Mode` (`app/keys.rs:241`).

| What | Drawn | Keys | State | Mode |
|---|---|---|---|---|
| Picker (every list) | `views/picker.rs:64` | `picker_key` `picking.rs:765`, then `Picker::key` `input/picker.rs:839`, which returns `Outcome::{Stay, Choose(Pick), Close, Tag}` (:365) | `App.picker` `app.rs:301` | Picker |
| Explorer and kit dialog: the picker as a tree | same draw (`KIT_EXPLORER_WIDTH` :70) | `input/picker/tree.rs` (marking :1005, :1033) | `Picker.tree` `input/picker.rs:464`; `App.kit` `app.rs:438-440` | Picker |
| Tag row (`T`) | `views/picker.rs:246-247` | `tree.rs:672` `tag_key`, then `tagging.rs:24` | `Picker.tag_line` | Picker |
| Command line | `views/status.rs:633` | `command_line.rs:15` | `App.command_line` `app.rs:318` | Command |
| Completion menu | `views/picker.rs:167` `draw_menu` | `command_line.rs:80` | `App.command_menu` `app.rs:320` (holds a `Picker`) | Command |
| which-key | `views/help.rs:19` | none of its own; rows from `popup_rows` `app/keys.rs:316` | `App.pending` `app.rs:409` | any, while keys are pending |
| Help, `Space k` | `views/help.rs:46` | `help_key` `app/keys.rs:391` | `App.help_scroll` `app.rs:402` | Help |
| Guide, `Space ?` | `views/guide.rs:28` | `guide_key` `app/keys.rs:436` | `App.guide*` `app.rs:404-407` | Guide |
| Shape panel | `views/shape.rs:81` | `shape_key` `shaping.rs:1310`, own typing at :1541 | `App.shape` `app.rs:428` | Shape. `mode_after_picker` (`shaping.rs:1182`) returns to it after a picker |
| Macro mapping, `gm` | `views/mapping.rs:39` | `mapping_key` `macroing.rs:497`, own typing at :565 | `App.mapping` `app.rs:430`, `mapping_image` :264 | Mapping |
| Field jump, `/` | labels inline through `widgets/setting.rs:37` | `jump_key` `jumping.rs:175` | `App.field_jump` `app.rs:434` | Jump |
| Restore prompt | status text, `views/status.rs:500` | `restore_key` `autosave.rs:60` | `App.status` | Restore |
| Row inspector, `Space i` | `views/inspector.rs:163`, placed by `views/phrase.rs:1435-1444` | none | `App.inspector` `app.rs:398` | none |
| Keys mode box | `views/track.rs:729-736` | `keys_key` `app/keys.rs:274` | `held_notes`, `octave` | Keys |
| Status, system line, meter column, scope | `views/status.rs:500`, `views/system.rs:47`, `views/meter.rs:89` and :62 | none | `App.status`, `cpu`, `meter` (`app.rs:341`), `scope` | always shown |

### 1.3 The shared widgets (`views/widgets/`, after #315 and #321)

| Widget | Where | Takes | Unit tests |
|---|---|---|---|
| `ValueBar` (fill or centred, set and live, macro range) | `bar.rs:29` | plain values | 4 |
| `setting::{text, style, jump_label, value_spans, name_line}` | `setting.rs:16-80` | `&Ctx` (since #715) | none |
| `SettingsGrid` and `CellSource` (4 columns by 3 rows, holes, `CellWidths::{FX, ZOOM, SOUND, COMPACT}`) | `grid.rs:22`, :35, :94 | a `CellSource` | none |
| `Device` (frame, title, switch, focus) and `rack_areas` | `device.rs:40` | plain values | none |
| `GraphicPanel` (boxes holding image placeholders or meter cells) | `panel.rs:36` | ids and lines | none |
| `Columns` (fixed column widths for `Table`s) | `table.rs:19` | widths | yes |
| `toggle`, `hint`, `group`, `role`, `meter` cells, `image` | their files | plain values | some |

### 1.4 The parameter UI in detail

**Knob grids** (FX, Sound, Mod and Gen pages). The cursor model is `DeviceSpec`
(`track_page/device.rs:30`), built from a model `Grid` (`vt-model/grid.rs:13`, 4×3). The pages
are `sound::devices`, `modulation::devices`, `fx::devices` and `generators::devices`, called from
`Page::devices_on` (`track_page.rs:85`). Each page draws through its own `CellSource`:

| `CellSource` | Where | Name | Value | Bar |
|---|---|---|---|---|
| `SlotCells` (FX and the zoom) | `views/track.rs:388-436` | `name_line(name_on)` | `value_line` (:313) | `moved_since` from the report (:417-428), `macro_range` (:432) |
| `SoundCells` | `views/track/sound.rs:145-243` | plus the taken-over glyph (:158-161) | coarse plus fine set back, a word, a slice's name, a tune owner (:166-213) | `bar_value` (:73); a fader's bar inverted (:232-236) |
| `SendsIn` | `views/track/sound.rs:466-501` | | | no live value, no range |
| `ModCells` | `views/track/modulation.rs:62-91` | | `value_line` with `at_default` | none |
| `MacroCells` | `views/track/generators.rs:58-110` | the macro's name | `{heard:04X}` | its own u16 copy of `moved_since` (:66-76) |
| `GenCells` | `views/track/generators.rs:150-217` | `GenParam::name` | `brief` or `shown` | its own 0..255 normalising (:193-196) and macro range (:199-209) |

- **Title controls** (the values in a device's title) have five copies: `views/track.rs:446`,
  `views/track/sound.rs:246`, `views/track/modulation.rs:105`, `views/track/generators.rs:237`,
  and inline in `views/track/zoom.rs:162-173`.
- **Value text** has two full per-`Control` formatters:
  - the cells' `views/track.rs:107` `value`, with `mod_value` at :210;
  - the status line's `views/status.rs:121` `value_shown`.
- **Edit keys.** `run_track_page`'s tail (`track_page.rs:661-679`) dispatches to:
  - `adjust_control` (`editing.rs:503`, a match over every `Control`);
  - `adjust_fine` (:763) and `set_fine_here` (:804);
  - `reset_control` (:816);
  - `open_control_picker` (`pickers.rs:138`, which fans out to 12 `open_*_picker`s);
  - Insert (`typing.rs:40`, `commit_control` :84).

**Settings rows.** These are label-and-value rows that don't use `SettingsGrid`:
- the project page, `views/project.rs:134` (with its own fine-digit Insert text at :157-158);
- the groove, :200-255;
- the audio page, `views/audio.rs:81` (a hand-made toggle under the cursor at :68-72);
- the sample page's settings line, `views/sample.rs:537-612` (cursor hard-coded to
  `theme::cursor(Mode::Normal)` at :546);
- the shape panel, `views/shape.rs:156-175`;
- the mapping view, `views/mapping.rs:86-131`.

Each of these has its own value formatter and its own cursor and dim rules. Their keys:
`project_page.rs:400` (`adjust_project`), `audio_page.rs:214/227/241` (three near-identical
on/off functions), `sample_settings.rs:379` (`change_setting`), and the raw-key handlers
`shaping.rs:1310` and `macroing.rs:497`.

**Mixer strips** (`views/mixer.rs`).
- The settings rows (:347-405) use `name_line`, `value_spans` and `setting::text`, but have their
  own six-way style chain (:382-390) instead of `setting::style`.
- The vertical fader is its own bar code, `fader_eighths`/`fader_cell` (:60, :69). Its heard mark
  shows from half a cell apart, where `ValueBar`'s needs a whole cell (:74 against `bar.rs:109`).
- Meter rows are at :253.
- State: `MixerPage.row`. Keys: as in 1.1.

**FX zoom** (`views/track/zoom.rs:70`).
- `CellWidths::ZOOM` with the shared `slot_frame` and `slot_grid`.
- Its own tile labels and values (:213-236) and a GR meter (:189-203).
- State: `TrackPage.zoomed` (`track_page.rs:173`). The caption is held twice: in
  `Graphics.zoom_caption` (`kitty.rs:1056`) and copied into `TrackPage.zoom_caption`
  (`kitty.rs:1647`, `track_page.rs:189`).

**Meters.**
- The data is `meter.rs` (`Held`, `Meter`). The cells are `widgets/meter.rs:16-85`.
- Where they're drawn: the main column `views/meter.rs:89`; the mixer `views/mixer.rs:253`; the
  Output device's across-meter `views/track/sound.rs:281`; GR in the rack `views/track.rs:618`;
  the zoom.
- State: `App.meter`, `App.track_meters` (through `Ctx::track_meter`, `page.rs:126`), and
  `TrackPage.{slot_reduction, reduction_history, after_trim}` (`track_page.rs:200-206`), which
  `App::tick` feeds. No keys.
- `bar.rs:25` `EIGHTHS` and `widgets/meter.rs:66` `LEFT_EIGHTHS` are the same table.

**Phrase and chain grids.** These are the grammar's grids, not parameter widgets: rows of steps
with fields, a Vim cursor (`App.cursor`) and operators.
- Drawn: `views/phrase.rs:817` and `views/chain.rs:18`.
- Keys: `grammar.rs` and `tree.rs:546`.
- Step locks show as marks at `views/phrase.rs:1363-1381`, and the inspector words them again at
  `views/inspector.rs:47-53`.

They share only value formatting with the parameter widgets (`step_text`, `views/phrase.rs:122`;
`phrase_value`, `views/status.rs:217`). They become page components under #320 and are not part of
the parameter family.

### 1.5 The duplication, counted

| Pattern | Copies | Where |
|---|---|---|
| A cell's name, value and bar, built by hand | 6 `CellSource` impls | 1.4 table |
| A value in a device title | 5 | 1.4 |
| A label-and-value row outside `SettingsGrid` | 8 | project, groove, audio, mixer, sample line, shape, mapping, group Tracks (`views/track/sound.rs:526-548`, which re-does the grid's placement by hand) |
| What's playing, compensated for the report's lag (`moved_since`) | 3 | `views/track.rs:417-428`, `views/track/sound.rs:221-231`, `views/track/generators.rs:66-76` |
| A macro's range for the bar | 2 | `views/track.rs:438`, `views/track/generators.rs:199-209` |
| Cursor style or Insert `_` done by hand instead of `setting::{style,text}` | 6 style, 5 text | `views/sample.rs:546`, `views/mapping.rs:100,108`, `views/shape.rs:161-169`, `views/project.rs:157`, `views/mixer.rs:382-390`, `views/audio.rs:68-73` |
| Per-`Control` value-to-text | 2 full, about 10 page-local | `views/track.rs:107`, `views/status.rs:121`; `views/project.rs:45`, `views/audio.rs:22`, `macroing.rs:74/98/112` |
| `if on {"on"} else {"off"}` | 9 | `views/track.rs:158,159,185,223`, `views/project.rs:47`, `views/status.rs:155,351`, `sample_settings.rs:137,144` |
| `TrackControl` to its track field (12 arms) | 6 | `views/track.rs:120-131`, `views/track/sound.rs:91-99`, `editing.rs:561-574` and `:823-836`, `roto/edits.rs:212-225`, `roto/devices.rs:512-531` (#726) |
| Edit-key dispatchers (`= - + _ x K i`) | 7 Cmd, 2 raw-key | 1.4 |
| Typed-text buffers | 3 | `App.entry`, `ShapePanel.typing` (`shaping.rs:114`), `MappingView.typing` |
| Two hex digits commit at once | 3 | `typing.rs:63-76`, `shaping.rs:1585-1590`, `macroing.rs:597-610` |
| `next_in` | 2 | `editing.rs:46`, `project_page.rs:200` |
| `open_*_picker` functions | 42 | `track_page/pickers.rs` (12), `picking.rs`, `sample_settings.rs`, `project_page.rs`, `audio_page.rs`, `macroing.rs`, `shaping.rs`, `tuning.rs` |
| Popup `Clear` + `window_border()` + title | 7 | `views/picker.rs:113-121` and `:184-195`, `views/guide.rs:40-44`, `views/help.rs:38-40` and `:53-57`, `views/shape.rs:247-251`, `views/mapping.rs:66-71` |
| Popup rectangle worked out inline | 8 | `views/picker.rs:86-91` (a plain subtraction, safe only because of the `.min` two lines above), `views/mapping.rs:48-53` (the same job written another way), `views/guide.rs:30-34`, `views/shape.rs:86-91`, `views/help.rs:32-37`, `views/picker.rs:178-183`, `views/help.rs:53`, `views/inspector.rs:179-182` |
| Image ids held in separate fields | 7 on pages, 3 on `App` | `TrackPage.{mod_images, filter_image, waveform_image, slot_images, zoom_image, gen_images}`, `App.{mapping_image, scope_image, sample_images}`, plus `Graphics::new_id` checking 10 holders (`kitty.rs:1172-1199`) |
| Centring decided | 3 ways | `Kind::centred` (`vt-model/settings.rs:289`, no callers), `views/track/sound.rs:86-87`, `views/track/generators.rs:197` |
| `Units` enums | 2 | `vt-model/settings.rs:53` and `vt-model/shape/params.rs:249` |

Two things look like bugs or inconsistencies. A behaviour-identical PR must not fix either, so each
is a separate question:
- The mixer draws `track_as_heard` without `with_macros` (`views/mixer.rs:354`,
  `automating.rs:98`), while the track page draws `Track::with_macros` (`views/track.rs:744`). A
  macro-held fader could read differently on the two pages. I didn't check whether a macro can
  hold a mixer row. *Since fixed, at the user's word:* a macro can hold `Level`, `Pan` and the
  sends, and the mixer now draws each strip from `automating::as_shown`, its macros and then its
  steps, as the track page does.
- A macro-held setting with no bar (a choice, a Mod-page value, a title control) shows no sign that
  a macro holds it. *Since added, at the user's word:* a faint dot after its value (decision 341).

---

## 2. Component architecture

### 2.1 What's already there, and works

- **`Ctx`** (`page.rs:29`): a read-only lens with named methods only. Its doc (`page.rs:1-8`)
  makes it the seam: "what a page may read of the program is what's here".
- **Pages as structs.**
  - `TrackPage::key(cmd, times, track) -> Outcome` (`track_page.rs:353`) is testable with no
    `App`. #715 kept it so.
  - `Outcome::{Done, Refused(&str), Passed}` (`track_page.rs:131`) is the "not mine" signal.
- **Overlays as components.** `Picker::key(Key) -> Outcome::{Stay, Choose(Pick), Close, Tag}`
  (`input/picker.rs:365`, :839) owns its state and returns a message that `App` acts on
  (`picking.rs:765`). That is the Elm "out message" pattern, already in use.
- **Immediate-mode widgets with a data protocol.** `SettingsGrid` reads cells through `CellSource`
  (`grid.rs:22`) and "knows nothing of the page it's on".
- **Model-level messages.** Every edit is an `Action` applied by `App::apply`, with undo and
  versioning. The update half of TEA is already pure and tested in vt-model.
- **One static binding table** (`input/bindings.rs:1120`), keyed by `Mode`. which-key
  (`Table::continuations`, :1246) and `Space k` (`Table::rows`, :1260) are generated from it.

### 2.2 Prior art

**ratatui `Widget` / `StatefulWidget`** [read]
- What it is:
  - `Widget::render(self, area, buf)` (ratatui-core 0.1.2, `widgets/widget.rs:70`);
  - `StatefulWidget { type State; fn render(self, area, buf, &mut State) }`
    (`stateful_widget.rs:124-133`);
  - `WidgetRef` and `StatefulWidgetRef`, behind a feature (`ratatui-0.30.2/src/widgets/widget_ref.rs:89`).
- What we take: per-frame value widgets. Use `StatefulWidget` only where a widget keeps its own
  scroll, as ratatui's `List` keeps `ListState`'s offset.

**ratatui component template and docs** [known]
- What it is:
  - `Component { register_action_handler, init, handle_events, handle_key_event,
    update(Action) -> Option<Action>, draw(&mut self, frame, area) }`;
  - `Action`s go over an mpsc channel, and the config maps keys to `Action`s;
  - the docs describe Elm, Component and Flux patterns.
- What we take: the `key`/`draw` pair, and `draw(&mut self)`. Not the channel: `Cmd` and `Action`
  already do its job synchronously.

**gitui** [known]
- What it is:
  - `Component { commands(&self, out, force_all), event(&mut self, ev) -> EventState, focused,
    focus, is_visible, hide, show }`;
  - `DrawableComponent::draw(&self, f, rect)`;
  - `EventState::{Consumed, NotConsumed}`;
  - a shared `Queue` of `InternalEvent`s for cross-component requests;
  - the command bar and help are generated from each component's `commands()`.
- What we take: `NotConsumed` is our `Passed`. The dynamic `commands()` isn't needed, because we
  don't grey out keys.

**helix** [known]
- What it is:
  - `helix-term/src/compositor.rs`:
    `Component { handle_event(&mut self, ev, cx: &mut Context) -> EventResult, render(&mut self,
    area, surface, cx), cursor, required_size }`;
  - `EventResult::{Consumed(Option<Callback>), Ignored(Option<Callback>)}`;
  - `Compositor { layers: Vec<Box<dyn Component>> }`, events go top down, rendering bottom up;
  - `Context { editor: &mut Editor, jobs, scroll }`;
  - which-key ("infobox") is generated from the keymap trie.
- What we take: `&mut Context` handed to components is the decided `&mut Services`; generated
  key help. The compositor we don't need (§2.4).

**yazi** [known]
- What it is:
  - widgets are built each frame from a `&Ctx` (`Root { cx }` impl `Widget`);
  - the keymap is per layer (`[mgr]`, `[tasks]`, `[input]`, `[confirm]`, `[help]`, `[cmp]`…);
  - help and which are generated from the keymap.
- What we take: the closest match to us. Immediate mode with a `Ctx`, keys per layer, generated
  help.

**zellij** [known]
- What it is: plugins are `load`, `update(Event) -> bool` (whether to redraw) and `render(rows,
  cols)`.
- What we take: little. Process isolation solves a problem we don't have.

**M8** [known]
- What it is: every field edits the same way. Edit with left/right is ±1, and with up/down ±0x10.
  Values are hex 00–FF. Option plus Edit clears. Views sit on a 2D map.
- What we take: the case for one parameter family. One step rule for every hex value.

**Elektron** [known]
- What it is: pages of 8 data-entry knobs drawn 2×4, pushing a knob for the other resolution,
  p-locked values highlighted while a trig is held, LFO destinations.
- What we take: the grid of N params per page, fine/coarse, and showing "set" against "heard".

**tracker-ce** [read]
- What it is:
  - `ModuleBase` (`core/interface/core/modulesBase.h:20-61`): `start`, `stop`, `update`,
    `initDisplayControls`.
  - Controls (`cDisplayControl`, `core/libs/NewDisplay/displayControls.h:10`) only draw. Each
    parameter is a hand-written pair, `changeParamsVolume`/`showParamsVolume`
    (`fIinstrEdit.cpp:1837`, `gInstrEdit.cpp:1567`).
  - Tables exist only for the sample editor's effects (`strEffectDisplayConstantParams`,
    `sampleEditor.h:28`, edited generically by `modParameter`, `fSampleEdit.cpp:1421`) and for
    synth patch metadata (`Patch.h:114-141`).
  - Input works by rebinding function pointers, with no bubbling.
- What we take: a warning. Its hand-written pairs are the drift #718 item 1 found, and the
  table-driven parts are the ones that needed no per-parameter code.

**picoTracker** (LGPT lineage) [read]
- What it is:
  - `UIField` (`Application/Views/BaseClasses/UIField.h`): `Draw`, `OnClick`, `ProcessArrow`,
    `ProcessEditArrow`, `ProcessClear`, focus.
  - `UIIntVarField(position, Variable&, format, min, max, xOffset, yOffset)`: arrows step by the
    small or large offset and clamp (`UIIntVarField.cpp:76-105`).
  - Variants: `UIBigHexVarField`, `UINoteVarField`, `UIIntVarOffField`, `UISortedVarList`,
    `UITempoField`.
  - `FieldView` holds the list of fields and the focus (`FieldView.h`).
  - `Variable` types are INT, FLOAT, BOOL, CHAR_LIST and STRING.
- What we take: **the closest prior art for §3.** One field family bound to a value description,
  steps as data, and a container that owns focus.

**Deluge** [read]
- What it is:
  - `MenuItem` (`src/deluge/gui/menu_item/menu_item.h`) with encoder actions,
    `renderInHorizontalMenu(slot)` and `getOccupiedSlots`.
  - The family is `Value<T>`, then `Number` (min, max, `RenderingStyle` NUMBER/KNOB/BAR), then
    `Integer`, `IntegerWithOff` and `IntegerContinuous`; and `Enumeration`, then `Selection`
    (FULL and SHORT options) and `Toggle`.
  - `HorizontalMenu` lays N items into slots with paging (`horizontal_menu.h:29-82`).
- What we take: a family split by kind of value, short and full names (our cell label against the
  status line's name), and a container that pages N items into fixed slots.

### 2.3 The recommendation

Five layers, each of which exists in some form today:

```text
App ─ owns ─┬─ Services        song + history (apply), engine handle and its last report,
            │                  pool and analyses, Graphics, settings, status, cursor, folds
            ├─ Modes           view, mode, Insert's entry, field jump, pending keys, count
            ├─ pages           TrackPage, MixerPage, ProjectPage, AudioPage, SamplePage,
            │                  SongView, ChainView, PhraseView   (impl Page)
            └─ overlays        Option<Picker>, Option<ShapePanel>, Option<MappingView>,
                               command line + menu, guide, help        (key/draw, by Mode)
widgets (no Ctx, no App):      ValueBar, ParamCell, SettingsGrid, Device, GraphicPanel,
                               Columns, popup frame, meter cells, toggle, hint
```

- **Services, after the split.** This follows the user's 2026-11-14 decision on #320. I'd add two
  things the decision didn't name:
  - the program's cursor, which every page reads and the mixer's `h`/`l` must move
    (`mixer_page.rs:139-150` can't do that through a read-only `Ctx`);
  - the folds (`Ctx::folds`).
  Both are shared state, not one page's.
- **`Ctx<'a>`** stays the read-only lens for drawing, named methods only, over `&Services` and
  `&Modes`. Keys get `&mut Services` (decided), plus `&Modes` to read.
- **Pages implement `Page`** (§2.5).
  - `App` dispatches with `match self.view`: static, exhaustive, no `dyn`.
  - Pages mutate their own state directly, apply edits as `Action`s through `Services`, and
    return `Outcome`.
  - What spans pages stays `App`'s: `i`'s mode change, `/`, `M`, `gm`, the Keys mode, opening a
    picker. A page returns `Passed`, and `App` handles these after it, as `run_track_page` does now
    (`track_page.rs:630-680`).
- **Widget against StatefulWidget.**
  - Value widgets are `Widget` over owned values, or over references through `impl Widget for &T`
    where a widget is reused.
  - `StatefulWidget` is only for a widget with scroll state of its own. The picker is the one
    candidate, and it already draws through `&mut Picker` (`views/picker.rs:64`).
  - No `WidgetRef`: it's still behind a feature in 0.30.
- **Messages against mutation.**
  - Take direct mutation for a page's own state.
  - Use `Action` for the song. Undo, autosave and the engine's versions all hang on it already.
  - Use out-messages only where an overlay finishes: `Picker`'s `Choose(Pick)`.
  - `Pick` collapses with §3.5. A described parameter's choice becomes `Pick::Param { control, value }`,
    in place of many variants.
  - No UI `Msg` enum. It would sit between `Cmd` and `Action` and duplicate both.
- **A key a component didn't handle.** It returns `Outcome::Passed`. The bubble order is fixed and
  should be written down in the Architecture section:
  1. the overlay for the `Mode`;
  2. the page;
  3. `App`'s cross-page commands;
  4. `run_tree`;
  5. `run_grammar`;
  6. the global `match`.

  This is gitui's `NotConsumed` and helix's `Ignored`, without callbacks.
- **Focus.**
  - Within a page, focus is the page's cursor (`TrackPage.row/field/fine/step`, `MixerPage.row`,
    `SamplePage.setting`). The page tells a widget `Focus::{Off, Value, Fine}` through its
    `CellLook`; widgets hold no focus.
  - Across layers, focus is `Mode`, which is also the binding table's key and which-key's scope.
    That's why it stays.
- **Services through `Ctx`.**
  - Pages read only through `Ctx`'s named methods. That's today's rule, and it's what makes a page
    testable with `Services` built for a test.
  - Widgets read nothing: the page computes a `ParamState` and a `CellLook` and hands them over.

### 2.4 What this costs, and what I rejected

- **Costs.**
  - Each remaining page is an M PR with only mechanical test renames, as #715 had.
  - The `Page` trait is a new abstraction. It has eight users, and it removes the copy, run,
    write-back and `Outcome` handling that `run_track_page` (`track_page.rs:605-623`) and
    `run_mixer` (`mixer_page.rs:134-140`) each repeat.
  - The copies themselves go away with the split. A page is then a field disjoint from `Services`,
    so `self.track_page.key(cmd, n, &mut self.services)` borrows cleanly.
- **Rejected: a helix-style compositor (`Vec<Box<dyn Component>>`).**
  - Overlays stack two deep at most: the picker over the shape panel (`shaping.rs:1182`) or the
    mapping view.
  - The binding table is keyed by `Mode`. Replacing `Mode` with a layer stack would re-plumb
    which-key, help and `key_scope` (`app/keys.rs:324`) for one special case.
  - `dyn` also needs `Any` downcasts to reach a layer's state, which helix does with
    `Compositor::find`.
- **Rejected: TEA's `Msg` and `update` for the UI.** `Cmd` comes out of the table and `Action` goes
  into the model. A third enum would only translate between them. The testable pure update already
  exists in vt-model.
- **Rejected: gitui-style dynamic `commands()`.** which-key never greys out a key, so a static table
  is simpler and keeps `Table::resolve` a lookup (§5).
- **Rejected: a `tick`/`update` trait method now.**
  - `App::tick` feeds `TrackPage`'s meters and nothing else page-side. One user, so it's an
    inherent `TrackPage::take_report(&Report)` until a second page needs one.
  - This follows #319's rule.

### 2.5 Sketch: the core traits

```rust
// pages/page.rs (today's page.rs, grown)

/// What a key did, as far as the component alone can tell. Moved from track_page.rs:131.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// It did it.
    Done,
    /// It can't be done here, and the status line says why.
    Refused(&'static str),
    /// It isn't one of its keys: the next in the bubble order gets it.
    Passed,
}

/// One of the program's pages: its own state, its keys, its drawing and its pictures. App holds
/// one of each beside the services, and dispatches by View, so there's no dyn and no downcast.
pub trait Page {
    /// Its rows of the binding table, as `(keys, Cmd, description)`, in the order which-key and
    /// `Space k` list them. The table is still assembled in one place (input/bindings.rs).
    const KEYS: &'static [(&'static str, Cmd, &'static str)];

    /// Run one of its keys `count` times, changing its own state directly and the song through
    /// the services' actions.
    fn key(&mut self, cmd: Cmd, count: usize, sv: &mut Services, modes: &Modes) -> Outcome;

    /// Draw it. It may note where it drew what the terminal places after the frame, an LFO's dot
    /// or a preview's playing row, as ratatui's List notes its scroll offset in its state.
    fn draw(&mut self, frame: &mut Frame, area: Rect, ctx: &Ctx);

    /// What pictures it shows this frame, as data, before ratatui draws (§4). Most have none.
    fn panels(&self, _ctx: &Ctx) -> Vec<PanelWant> {
        Vec::new()
    }

    /// The ids `Graphics` gave those pictures, for `draw` to put placeholders for.
    fn take_panels(&mut self, _ids: PanelIds) {}

    /// What the terminal places itself once the frame is drawn: dots, lines, marks.
    fn placements(&self, _ctx: &Ctx) -> Vec<Placed> {
        Vec::new()
    }
}

impl App {
    /// Hand a key to the page on screen, then handle what it passes back.
    fn run_page(&mut self, cmd: Cmd) -> bool {
        let count = self.modes.count.take().unwrap_or(1).max(1);
        let (sv, modes) = (&mut self.services, &self.modes);
        let outcome = match self.modes.view {
            View::Track => self.track_page.key(cmd, count, sv, modes),
            View::Mixer => self.mixer_page.key(cmd, count, sv, modes),
            View::Project => self.project_page.key(cmd, count, sv, modes),
            // … every View, exhaustively
        };
        match outcome {
            Outcome::Done => true,
            Outcome::Refused(why) => {
                self.services.say(why);
                true
            }
            Outcome::Passed => self.run_across_pages(cmd, count),
        }
    }
}
```

Overlays keep their own signatures, which differ in what they return, as `Picker::key` does. They
share only the popup frame and rectangle helpers:

```rust
// widgets/popup.rs: the 7 copies of Clear + window_border + title, and the 8 inline rects.
pub fn popup(frame: &mut Frame, area: Rect, title: Line<'static>) -> Rect; // clears, frames, gives the inside
pub fn centred(area: Rect, width: u16, height: u16) -> Rect;              // saturating, never underflows
pub fn right_of(area: Rect, width: u16, height: u16, from_top: u16) -> Rect;
pub fn above(status: Rect, column: u16, width: u16, height: u16) -> Rect;
```

---

## 3. Parameter widgets

### 3.1 What a widget needs from the device description

The device study (`device-interface.md`) came in while I was finishing this one. Its §2.5
already gives the widgets a contract:
- **`ParamView`**: one resolved value per setting, with `key`, `name`, `label`, `value: i32`,
  `fine`, `bounds`, `default`, `nudge`, `jump`, `wraps`, `centred`, `toggle`, `choices`, `text`,
  `typing`, `units` and `engaged`;
- **one resolver**, `fn view(control, track, song) -> Option<ParamView>`, to be written in its PR
  S2.

This study builds on those names. The table checks each thing the widgets need against
`ParamView`. Four rows are gaps the device study should close (the last column), or the widgets
can't render identically.

| The widget needs | Today | In `ParamView` | Gap |
|---|---|---|---|
| a short label for the grid, a full name for the status line | `Entry.label`/`name` (`vt-model/settings.rs:459`), `Param.label`/`name` (`vt-model/insert.rs:316`), `Control::label`/`name_on` (`controls.rs:204`, :288), `GenParam::name` | `label`, `name` | – |
| the face: hex, hex with fine, word, toggle, number | `Kind` (`vt-model/settings.rs:40`), `Param.choices` + `is_toggle` (`vt-model/insert.rs:341`), `Control::is_hex`/`is_choice` (`controls.rs:477`, :490) | derivable from `toggle`, `choices`, `fine` and `typing` (`Hex` means hex) | – |
| **the cell's text, which isn't the status line's** | cells show hex `80` for a continuous value (`views/track.rs:107`) where the status line shows `1.21 kHz` (`views/status.rs:121`, `Kind::shown` `vt-model/settings.rs:295`); a generator's cell shows `brief` when it has a bar and `shown` when not (`views/track/generators.rs:176-178`) | one `text`, "in real units" | **Add `cell: String`** (or the rule "hex for `Typing::Hex`, `text` otherwise", plus `GenParam::brief`). The context-dependent cases (an attractor's `x`/`y`/`z`, the note-shaping slot's `--`, `views/track.rs:226-281`) are covered, because `view` resolves with the track |
| **steps that depend on the value and on direction** | `time_step` (`editing.rs:53`): 1, 5 or 25 ms by the value, and going down uses `time_step(value - 1)` (`editing.rs:71-78`), so up and down differ at 10 and 100; an attractor's large phase step is 120° rather than 90° (`editing.rs:31`) | `nudge`, `jump`, resolved at the current value | **`nudge` and `jump` each way** (`nudge_down`, `jump_down`), or a `Step` rule (§3.3). Resolving them at the current value covers the attractor |
| the default, and whether it's at it | `Entry.default`, `Param.default`, `at_default` (`views/track.rs:94`), `mod_at_default` (`editing.rs:241`) | `default`, `engaged` | – |
| its bar | `bar_value` (`views/track/sound.rs:73`), `SlotCells::bar`, `GenCells::bar` | `centred`, plus `value` and `bounds` for the position, as `GenCells` places it (`views/track/generators.rs:193-196`) | – |
| typing | `Control::is_hex`; `Typing` (`vt-model/shape/params.rs:226`) | `typing` | – |
| **`K`'s list, labelled as today** | three styles: plain (`pickers.rs:262-278`), numbered `"{at:02X}  {choice}"` (`pickers.rs:480`), and with a meaning (`pickers.rs:288`) | `choices: Choices` (the `Listed` trait carries meanings) | **Whether a list is numbered** |
| **writing it back** | 6 copies of the `TrackControl` match; `set_hex` (`editing.rs:473`); `Insert::with_value` | nothing: `ParamView` is read-only | **A companion `fn set(control, value, fine) -> Option<Action>`** beside `view`, built on M1's `ModSlot::set_field` and `Track::set_control` |
| grouping into devices | `Insert::grid`, `Machine::grid`/`operators` (`vt-model/song.rs:1028`, :1065), `GenParam::grid` (#724) | on the device, not the param: `Grid` and `Place` | – |

The value scale is `ParamView`'s `value: i32` within `bounds`, plus `fine: Option<u8>`. The bar's
0..255 position is worked out from those once, in the widget. The live report's `u8`s
(`LiveValues`) feed `heard` on that same scale.

**As U7 built it** (`track_page/view.rs`). The four gaps are closed as `cell: String` beside
`text`, `nudge` and `jump` as `Steps { up, down }`, `numbered: bool`, and `set(control, at,
track, song, value, fine) -> Option<Action>`. Three things differ from the sketch:
- `label` is the cell's name (`Cow<'static, str>`, as a macro's is the name it was given), not
  the registry's lockable label.
- The bar's place is `bar: Option<u8>`, resolved in `view`, because the level-scale settings'
  bounds stop at their top while their bars run to `FF`, and a list of a machine's has bounds
  but no bar.
- `view` takes the scale lock as a fourth argument, which an accumulator's limits are counted
  in, so it lives in vt-tui; `Ctx::param_view` passes the program's.

A macro's cell shows where it's heard, from the report, so that one text stays the page's.

### 3.2 `ParamCell`: one widget, five faces

A cell is a name, a value and a bar, as `SettingsGrid::render` lays them out (`grid.rs:119-163`).
`ParamCell` builds those three parts from the description, a `ParamState` (what the page knows
now) and a `CellLook` (what the modes say). It **doesn't format text**. The description does, and
the page passes the text in, so the widget stays free of `Track` and `Ctx`.

| Face | Today | Drawn as |
|---|---|---|
| `Hex` | FX params, machine settings, levels | `80`, bar if the kind has one |
| `HexFine` | Sound page `Base`/`Setting` with a fine part (`Control::fine_on`, `controls.rs:413`) | `A0` padded to `COARSE_WIDTH`, then the fine `00` dim (`views/track/sound.rs:186-195`) |
| `Word` | choices, note divisions (`1/16`, an LFO's or arpeggio's rate), `Off`, `G1`, a slice number with its name after | the word across the bar's room (`grid.rs:154-161`) |
| `Toggle` | `OFF_ON` params, `Safe`, `Limiter`, `Reset`, `Retrig` (`views/track.rs:295`) | `toggle::text(on)` in `toggle::style(on)`, and the cursor's style under the cursor (`views/track.rs:322-332`) |
| `Number` | Mod page values: `120 ms`, `50%`, `90°`, `Note` | text from the description, no bar (`CellWidths::COMPACT`) |

The rendering rules come from today's code, and each is one function in the widget:

1. **Name.** `name_line(label, labels, room)` (`setting.rs:58`), dim, with the jump labels over the
   letters after the query. Where `state.taken`, add `" "` and `glyphs::TAKEN` in
   `theme::taken()` (`views/track/sound.rs:158-161`, `views/mixer.rs:398-401`).
2. **Value text.**
   - While Insert is on this part, it's the typed text plus `_` (`setting.rs:16`).
   - Otherwise it's `view.cell`, the cell text §3.1 asks the device study for (today `value`, `views/track.rs:107`).
   - With a value label from a jump, the label replaces the first letter (`setting.rs:44`).
3. **Value style**, in this order:
   - the cursor's style when focused (`theme::cursor(mode)`);
   - then `Tone::Tint(style)` (a group's colour or a role's, `views/track/sound.rs:113-123`);
   - then `Tone::Automated` (`theme::automated()`);
   - then `Tone::Default` dim;
   - then a toggle's own colour;
   - otherwise plain.

   That's `setting::style` plus `value_style`, merged. `Tone::Empty` is faint (`goes_nowhere`,
   `views/track/modulation.rs:96`, and an empty slot's `Empty`).
4. **Fine part.** Under `HexFine`, `{fine:02X}` in `setting::style(focused_on_fine, true)`: dim
   unless focused.
5. **After.** A span in the bar's room, padded to `VALUE_WIDTH + 1` or `TUNE_WIDTH`
   (`views/track/sound.rs:196-211`): a slice's name in mauve, a tune's owner.
6. **Bar.**
   - `ValueBar::new(set, cells).playing(heard)`, centred if `view.centred`, and
     `.range(state.macro_range)`.
   - A fader (decision 174) swaps `set` and `heard` (`views/track/sound.rs:232-236`).
   - None for `Word`, `Toggle` and `Number`.
7. **Faded.** A switched-out device's lines are faint except the cursor's style (`grid.rs:166`).
   This stays the grid's job.

`heard` comes from one helper, which replaces the three copies:

```rust
/// What a value plays with now, as the report says it, moved by what the setting has moved since
/// that report's version (#421), or what's set when there's no report of the same device.
pub fn heard(set: u16, live: Option<u16>, then: Option<u16>) -> u16; // moved_since, in u16
```

**Locks, modulation and macros on the page.** The widget never needs to know *why* what's heard
differs from what's set. A step's lock, an LFO, a macro and an automated fader all show the same
way: the fill follows `heard` and a dim mark stays at `set`. The reason shows through separate
state:

| Reason | Shown as | Today |
|---|---|---|
| Steps drive a fader | `Tone::Automated` | `follows_steps`, `views/track/sound.rs:120`, `views/mixer.rs:388` |
| A hand has taken a fader over | `taken` | `taken_over` |
| A macro holds it | `macro_range` (░ behind the fill) | `macro_range` |
| A slot's bypass is locked | the device fades | `heard_bypassed`, `views/track.rs:379` |

The page itself is drawn from `Track::with_macros`, so `set` is already where the macro puts it.
A macro-held cell with no bar keeps showing nothing extra, as today. Changing that would be a
feature. *Since added, at the user's word (decision 341):* such a cell has a faint `•` after its
value, from the same `macro_range`.

### 3.3 Editing: `param::edit`, pure functions

These read only `ParamView`'s fields, so they know no device:

```rust
// widgets/param/edit.rs

/// Where `= - + _` take a value: `nudge` or `jump` that way, `count` times, clamped to `bounds`,
/// or round them where it `wraps`. A list moves one choice at a time whatever the size, as the
/// filter type does today (editing.rs:599), so a large step can't skip through a short list.
pub fn stepped(view: &ParamView, up: bool, large: bool, count: usize) -> i32;

/// The fine byte moved by one or 0x10, as adjust_fine does (editing.rs:763).
pub fn stepped_fine(view: &ParamView, up: bool, large: bool, count: usize) -> u8;

/// What `i`'s text sets it to, under its `Typing`, or why not, for the status line.
pub fn typed(view: &ParamView, text: &str) -> Result<i32, String>;

/// What `x` puts back.
pub fn reset(view: &ParamView) -> i32 {
    view.default
}

/// `K`'s list, opened on the value it has, or none for a value that isn't a choice.
pub fn choices(view: &ParamView) -> Option<ChoiceList>;
```

If the device study goes with a `Step` rule instead of directional nudges (§3.1), `stepped`
matches on it:

```rust
pub enum Step {
    Linear,                          // nudge and jump, clamped: hex 1 and 0x10, a percent 1 and 10
    Time,                            // an envelope's 1, 5 or 25 ms by how long it is, down from below
    Wrap,                            // round a cycle: a phase's 15° and 90°, an attractor's 120°
    List { wraps: bool },            // one choice at a time either way
}
```

The page wires them up once, for every setting `view` resolves:

```rust
Cmd::Increase | Cmd::Decrease | Cmd::IncreaseLarge | Cmd::DecreaseLarge => {
    let control = self.control(sv.track_here());
    let Some(view) = view(control, sv.track_here()?, sv.song()) else { return Outcome::Passed };
    let up = matches!(cmd, Cmd::Increase | Cmd::IncreaseLarge);
    let large = matches!(cmd, Cmd::IncreaseLarge | Cmd::DecreaseLarge);
    let (value, fine) = match self.fine {
        true => (view.value, Some(edit::stepped_fine(&view, up, large, count))),
        false => (edit::stepped(&view, up, large, count), view.fine),
    };
    match set(control, value, fine) {
        Some(action) => sv.apply(&action),
        None => return Outcome::Passed, // a Control `view` doesn't cover yet keeps its own arm
    }
    Outcome::Done
}
```

Some things the generic path can't cover. Those `Control`s keep their own arms in the page until
the description covers them:
- the sample under `Sample`, which steps through the bank (`editing.rs:1017`);
- `Tune` and `Cents`, which write the bank entry (`tuning.rs:173`);
- `Mute` and `Solo`, which are live, not song;
- a macro-held refusal (`macroing.rs:206`), which stays a check before the dispatch.

The order of migration (§8) moves the two families that are already table-driven first: effect
params (`Param`) and machine settings (`Entry`). `adjust_control` already handles both generically
(`editing.rs:539-543`, :580-591). After them come the mod fields, the accumulator, the arpeggio and
the track controls, as the description reaches them. That last group is #726.

### 3.4 Typing: `TypedValue`

There are three buffers today (`App.entry`, `ShapePanel.typing`, `MappingView.typing`), with three
copies of the two-hex-digit commit. Replace them with one component, driven by the description's
`Typing`:

```rust
pub struct TypedValue { text: String }

pub enum Typed { Editing, Commit(String), Cancel }

impl TypedValue {
    /// A key typed into it under a rule: two hex digits commit as the second is typed, others on
    /// Enter, Esc cancels, Backspace takes one back, and what a rule doesn't allow is ignored.
    pub fn key(&mut self, code: KeyCode, rule: Typing) -> Typed;
    /// What it shows in place of the value: the text and a `_` where the next character goes.
    pub fn shown(&self) -> String;
}
```

The behaviour stays as today's, including the differences that exist:
- the Gen page only filters hex from not-hex (`typing.rs:44-46`);
- the shape panel filters by `Typing` variant (`shaping.rs:1556-1598`).

The first PR keeps both behaviours behind the rule each caller passes. Making them agree is a
behaviour change for later.

### 3.5 Choices: one picker builder

```rust
pub struct ChoiceList { pub title: &'static str, pub items: Vec<(String, &'static str)>, pub at: usize }

/// The picker for a described choice, opened on the current one, each item picking `value`.
pub fn choice_picker(control: Control, list: ChoiceList) -> Picker; // items: Pick::Param { control, value }
```

This keeps today's three label styles as data on the description: plain (`pickers.rs:262-278`),
numbered `"{at:02X}  {name}"` (`pickers.rs:480`), and with a meaning (`pickers.rs:288`). The pickers
that list things that aren't choices of a value stay their own: grooves, samples, presets,
patches, the macro picker.

**As U8 built it** (`widgets/param/edit.rs`, `track_page/pickers.rs`). The view gained what the
edits read: `pace` (`Even`, `Once` for the track's own lists and an effect's, which a count has
never moved further, `Time`, `Hold` and `Bars`, whose steps are worked out again after each step
of a count), `reads` and `why_not` for `typed`, `title` for `K`'s list, and `choices` as `Choice`s
with a meaning and a heading. `stepped_fine` gives the value as well as the fine byte, as a fine
step carries. A machine's own lists still step as hex, `+` moving `10` and stopping at the last,
as they did. `choice_picker(track, control, list)` replaces ten of the pickers; the machines'
glyphs, the roles' colours and the presets after a machine or a slot are added by the control.
The mod slot's type, rate, fade and reset, the groove, the sample, slice and tune lists and the
macro's patches stay their own, as do the 30 of the 42 `open_*_picker`s that aren't the track
page's settings. A Roto knob's turn writes through `set` too, with `edit::shared` and
`edit::places` for where its position lands.

### 3.6 The grid and page container

The container exists. It is three steps, and the gaps are small:

1. **A device's places.** `Grid` (`vt-model/grid.rs:13`, 4×3, holes). Gaps:
   - `GenParam::grid` still returns raw arrays, and `generators.rs:116-145` transposes them by hand
     (#724);
   - the Sound page's Filter/Output/Play and the Mod page's grids are `Control` tables in vt-tui
     (`track_page/sound.rs:49-57`, `track_page/modulation.rs`).
2. **The cursor's model.** `DeviceSpec` (`track_page/device.rs:30`), with `rack_places`,
   `device_above_below` and `rack_column`. It has no gap. N parameters already become several
   devices: 4-op's shared settings and its four operators (`track_page/sound.rs:31-36`).
3. **Drawing.** `SettingsGrid` plus `CellSource` (`grid.rs:22`, :94). The gap: each page writes its
   own `CellSource`.

The changes:

```rust
impl DeviceSpec {
    /// A device's entries in a rack: one for each of its groups, each of up to twelve settings in
    /// the group's Grid, the first with the title controls. 4-op is five (its shared settings,
    /// then Op 1 to Op 4). The device study has no one device type for all four families, so this
    /// takes what each spec gives: its grids by `Place::Own`, its title by `Place::Type`/`Title`.
    pub fn from_groups(
        identity: &str,
        title: [Option<Control>; DEVICE_COLUMNS],
        groups: impl IntoIterator<Item = (String, Grid)>,
        control: impl Fn(usize) -> Control,
    ) -> Vec<DeviceSpec>;
}

/// What a grid's cells show. One method, so a cell's parts are worked out once, not three times.
pub trait CellSource {
    type Id: Copy;
    fn cell(&self, id: Self::Id, widths: CellWidths) -> CellParts;
}

pub struct CellParts {
    pub name: Line<'static>,
    pub value: Line<'static>,
    pub bar: Option<ValueBar>,
}
```

- **One `TrackCells` replaces the six impls.** It is a `CellSource` over `Control` for the whole
  track page. For each `Control` it works out the description, a `ParamState` (from
  `track_as_heard`/`with_macros`, the report, the macro map and the faders) and a `CellLook` (from
  the page cursor and `Ctx`'s modes), and calls `ParamCell::parts`.
- **The title controls** become the same call with a `Word` face and no bar.
- **Width** stays the page's choice: Full for FX, Sound and Gen; Half for the Mod page's slots.
- **Paging past a screen** stays as it is: the rack goes by whole bands to keep the cursor's
  device in view (`widgets/device.rs`, `rack_areas`). "Limits don't drop features" is already met
  by more devices, not by fewer parameters.
- **The Roto's pages** (`roto/layouts.rs`) stay hand-written. Knob order and the stable `kind`
  strings are hashed onto the device (#419 and the header at `roto/layouts.rs:1-37`). What changes
  is only their lookups:
  - `machine_layout`/`effect_layout`/`generator_layout` (`roto/layouts.rs:2007-2236`);
  - the `key()` table (:2363-2455);
  - `resolve` (:2315);
  - the knob lists in `roto/devices.rs` and `roto/edits.rs`, which mirror `ModField` and
    `TrackControl` by hand.

  These resolve a key through the description to the same `Control` the screen uses, and `view` gives its `ParamView`. A knob turn
  becomes `edit::stepped` and `set`, the same path as a key. That is #731 plus #726. The
  screen and the Roto then share facts and keep separate layouts, which is #419's intent.

### 3.7 Rendering identically

- The screenshots (`examples/screenshots.rs`, SVGs committed under `docs/manual/src/images/`) catch
  what they show. They don't show:
  - every machine and effect;
  - typing in every face;
  - jump labels on every page;
  - a fader taken over;
  - a macro range on the FX page.
- **Before the refactor**, the widget tests (§6) pin `SettingsGrid`, `setting` and `device` as they
  are now.
- **During it**, a temporary parity test in `tests/` draws, for a table of tracks, every `Control`
  through the old `CellSource` and the new `TrackCells` and asserts the `Line`s are equal, styles
  included. The tracks cover every machine, every effect and every mod type, with and without a
  report, with a macro, typing, a jump and a taken fader. The PR deletes the old path and the
  parity test together once both agree, or the next PR does.
- Every PR states: `just screenshots` gives no diff, and `just ci` passes.

### 3.8 Sketch: the widget

```rust
// widgets/param.rs

/// How a value reads in its cell, from its description's kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face { Hex, HexFine, Word, Toggle, Number }

/// Where the cursor is on a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Focus { #[default] Off, Value, Fine }

/// How a value is coloured when the cursor isn't on it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tone {
    Plain,
    /// Left at its default, or doing nothing on this track: dim.
    Default,
    /// A fader the steps drive now.
    Automated,
    /// A group's colour, a role's.
    Tint(Style),
    /// Nothing there, as an empty slot or a destination of `---`: faint.
    Empty,
}

/// What the page knows of one setting now that its `ParamView` can't: worked out by the page
/// from the engine's report, the macros and the faders, never by the widget.
#[derive(Debug, Clone)]
pub struct ParamState {
    /// What it plays with now, on the bar's 0..255, from `heard()` (#421).
    pub heard: u8,
    /// Filled to the fader with a mark at what's heard, rather than the other way (decision 174).
    pub fader: bool,
    pub macro_range: Option<(u8, u8)>,
    /// How it's coloured when the cursor isn't on it. `Default` follows `!view.engaged` unless a
    /// page has something stronger: a group's tint, the steps driving it, an empty slot.
    pub tone: Tone,
    /// A hand has taken this fader over from the steps.
    pub taken: bool,
    /// In the bar's room after the value, and the room it's padded to: a slice's name, whose
    /// tune it is.
    pub after: Option<(Span<'static>, usize)>,
}

/// What the modes say about a cell: the cursor, Insert's text, a jump's labels.
#[derive(Debug, Clone, Copy)]
pub struct CellLook<'a> {
    pub focus: Focus,
    pub mode: Mode,
    /// What Insert has typed, while it's on this cell.
    pub typed: Option<&'a str>,
    /// A field jump's query and the labels it puts over the name, and over the value.
    pub query: Option<&'a str>,
    pub name_labels: &'a str,
    pub value_label: Option<char>,
}

/// One setting's cell: what it is (the device study's ParamView), how it is now, how it's seen.
pub struct ParamCell<'a> {
    pub view: &'a ParamView,
    pub state: ParamState,
    pub look: CellLook<'a>,
}

impl ParamCell<'_> {
    /// How its value reads, from the view alone.
    pub fn face(&self) -> Face {
        let view = self.view;
        match () {
            _ if view.toggle => Face::Toggle,
            _ if !matches!(view.choices, Choices::None) => Face::Word,
            _ if view.typing == Typing::Hex && view.fine.is_some() => Face::HexFine,
            _ if view.typing == Typing::Hex => Face::Hex,
            _ => Face::Number,
        }
    }

    /// Its name, value and bar, for a grid to place in cells of these widths. Every rule here is
    /// one of today's, at the file and line §3.2 gives.
    pub fn parts(&self, widths: CellWidths) -> CellParts {
        CellParts {
            name: self.name(widths.name),
            value: self.value(),
            bar: self.value_bar(widths.bar_cells),
        }
    }

    fn name(&self, room: u16) -> Line<'static> { /* name_line(label) + TAKEN */ }
    fn value(&self) -> Line<'static> { /* typed_ | toggle | view.cell, + fine, + after; §3.2's style */ }

    /// Where the value sits on a bar, 0 to 255, from its place in its bounds.
    fn place(&self) -> u8 {
        let (low, high) = self.view.bounds;
        let share = f64::from(self.view.value - low) / f64::from((high - low).max(1));
        (share * 255.0).round().clamp(0.0, 255.0) as u8
    }

    fn value_bar(&self, cells: usize) -> Option<ValueBar> {
        if cells == 0 || !matches!(self.face(), Face::Hex | Face::HexFine) {
            return None; // a compact cell, a word, a toggle, a number
        }
        let (set, heard) = (self.place(), self.state.heard);
        // `new` is where the dim mark goes and `playing` what the fill follows, as ValueBar has
        // them (bar.rs:29-110). A fader is filled to what's set (views/track/sound.rs:232-236).
        let (marked, filled) = match self.state.fader {
            true => (heard, set),
            false => (set, heard),
        };
        Some(
            ValueBar::new(marked, cells)
                .playing(filled)
                .centred(self.view.centred)
                .range(self.state.macro_range),
        )
    }
}

/// A cell on its own, outside a grid, as a settings row draws one: name, value and bar in a row.
impl Widget for ParamCell<'_> { /* lays out parts() in CellWidths::SOUND, as SettingsGrid does */ }
```

`ValueBar`'s own names are `set` (the dim mark) and `live` (the fill). A fader passes what's heard
as `set` (`ValueBar::new(live, BAR_CELLS).playing(value)`, `views/track/sound.rs:233`). The sketch
keeps exactly that call, named for what each argument draws.

**`place` must match each page's maths exactly.**
- A hex value's position is the byte itself. Its bounds are `(0, 255)`, which `place` maps to
  itself.
- A level-scale value's top is `MAX_LEVEL`. Its bounds must stay `(0, 255)` for the bar, as today
  (`ValueBar::new(track.level, ..)`, with the top only clamping the steps). Otherwise the bar would
  move.
- A generator's position is `GenCells`' rounding (`views/track/generators.rs:193-196`).

The parity test (§3.7) is what catches a difference here.

**Settings rows outside the track page.** The project, audio and sample rows and the mixer's
rows are the same parts in another layout: a `Columns` table, or one line of spans on the sample
page. They adopt `ParamCell::parts` when their page becomes a component (§8). Their own rules are
kept, as `Tone`s and a `Focus`:
- the mixer's `waiting` glyph and muted dim;
- the sample line's inherited dim.

**The shape panel and the mapping view.** These show `GenParam`s and macro ends. They adopt
`ParamCell` with `Face::Word`/`Number`, and `TypedValue` for their buffers.

---

## 4. The kitty graphics side

**Today.**
- `Graphics::sync(&mut self, app: &mut App, out)` (`kitty.rs:1204`) runs before ratatui draws
  (`run.rs:453`). It reaches into pages through `App`:
  - `sync_sound` (:1557), `sync_waveform` (:1599), `sync_fx` (:1630), `sync_mod` (:1988) and
    `sync_gen` (:1691) for the track page;
  - `sync_mapping` (:1722), `sync_scope` (:1753), `sync_sample` (:1780) and `sync_preview` (:1510)
    for the rest.

  Each writes ids back into `app.track_page.*`.
- `Graphics::place` (:1240) runs after the draw. It places the sample page's marks, playheads and
  cursor line, the LFO dots (from `TrackPage.lfo_cycles_at`, which `draw_screen` stores at
  `run.rs:606-608`) and the switches.
- The panels' cache is `PanelImages` (`kitty/panels.rs`), by `(PanelKey, Shade)`, with `Shape`
  compared to decide whether to send again. Five callers do the same
  `self.panel(out, (key, shade), shape, size, |size| plot::…(..).png(shade.color()))`.

**Proposal: a page describes its pictures, `Graphics` sends them.**

```rust
/// A picture a page wants on screen this frame.
pub struct PanelWant {
    pub key: PanelKey,         // already the namespace: Filter, Waveform, Mod(slot), Slot(slot), Zoom, Gen(i), Mapping
    pub shade: Shade,
    pub shape: Shape,          // what it's drawn from, compared to tell if it's new
    pub cells: (usize, usize), // its size in characters
}

/// The ids of a page's pictures, by panel, for its drawing to put placeholders for.
#[derive(Debug, Clone, Default)]
pub struct PanelIds(BTreeMap<PanelKey, u32>);

impl Graphics {
    /// Send whichever of these the terminal lacks, at most one change a panel in PANEL_FRAMES, and
    /// give back each one's id. Retired ones are deleted next frame, as now.
    pub fn panels(&mut self, wants: &[PanelWant], rate: u32, out: &mut impl Write) -> io::Result<PanelIds>;
}

impl Shape {
    /// Its picture at these pixels in this shade: the closures sync_* pass today, as one match.
    pub fn png(&self, pixels: (usize, usize), shade: Shade, rate: u32) -> Vec<u8>;
}
```

The frame loop becomes:

```rust
let wants = app.page_panels(); // match view => page.panels(&ctx)
let ids = graphics.panels(&wants, rate, &mut out)?;
app.page_take_panels(ids);
terminal.draw(…)?;
graphics.place(&app.page_placements(), &mut out)?;
```

The screenshot example (`examples/screenshots.rs:550-565`) makes the same three calls.

**How a component owns its images**, then:
- **The page owns** its picture wants as a pure function of its state and `Ctx`, testable with no
  terminal. It also owns the `PanelIds` it was given and where it drew what carries a dot (as
  `Drawn`/`CyclesAt` today, `views/track/modulation.rs:56`).
- **`Graphics` owns** the protocol, id allocation, the cache by shape, rate limiting, retiring on a
  resize or cell change (`kitty.rs:1137-1170`), and placements.
- **Widgets own nothing.** `GraphicPanel` takes an id and draws placeholders, as now.

**What it removes.**
- Seven id fields on `TrackPage` (`track_page.rs:175-195`), `App.mapping_image`, and the double
  zoom caption: the caption becomes the page's, worked out from the `SlotDrawing` it wants.
- The 18 `&App` reads in `Graphics::sync`.
- The five `self.panel(…)` call shapes, which become one `Shape::png`.

**Also.**
- `Graphics::new_id` (`kitty.rs:1172-1199`) checks 10 holders by name. Give the protocol layer one
  set of live ids. Allocation must stay in the same order, so ids and the screenshots don't move.
- Split `kitty.rs` (2,107 lines) into `graphics/protocol.rs` (1-700), `graphics/slots.rs`
  (`SlotDrawing`, about 700-1000) and `graphics/mod.rs` (`Graphics`). This is #728's second half.

**Costs.**
- `Shape::Filter` must carry everything `plot::filter` reads. Today it is handed a `Track` with
  cutoff and resonance swapped (`kitty.rs:1574-1584`). `FilterSettings` already has the eight
  fields, so `plot::filter` needs a `FilterSettings` entry point.
- The scope (the meter column's), the picker's preview and the sample page's waveform are not track
  page panels. They stay `Graphics` methods called with what they need: `App.scope`, the picker,
  the `SamplePage`. That way `Graphics` still reads no `App`.

---

## 5. Keys, which-key, help and guides

**Today.**
- One static table (`input/bindings.rs`).
- Per-page slices: `TRACK_PAGE` :536, `MIXER` :979, `PROJECT` :949, `AUDIO` :932, `SAMPLE` :695,
  `SONG` :640, `CHAIN` :497.
- `bindings()` (:1120) assembles them under each page's `Mode` scope (`key_scope`,
  `app/keys.rs:324`).
- which-key and `Space k` are generated from it.
- The guides (`Space ?`) are hand-written Markdown that the manual includes. The overlays' raw-key
  handlers (shape, mapping, picker) aren't in the table; their keys show as hints on their window
  borders (`widgets/hint.rs`).

**Recommendation.**
1. **Each page declares its keys: `Page::KEYS`.**
   - Move each slice, unchanged, into its page module. `bindings()` assembles
     `TrackPage::KEYS`, `MixerPage::KEYS` and the rest in today's order.
   - A test pins the assembled table, mode, keys, `Cmd` and description row by row, so the move
     changes nothing.
   - This is gitui's and yazi's "component declares its keys", without gitui's per-frame dynamic
     list.
2. **No shared parameter key rows.** The descriptions of `= - x K` already differ per page:
   - "Increase the value" (:625);
   - "The next choice" (:939);
   - "Increase the value, or switch on" (:962);
   - "Switch mute or solo" (:995).

   Sharing would change `Space k`. The shared part is `param::edit` underneath, not the rows.
3. **Keep the table static.** which-key never shows a key as unavailable, and `Table::resolve` is a
   plain lookup. A `fn keys(&self)` per state would buy nothing and would cost the check that every
   name parses (`Table::new`, :1214).
4. **Guides.** Leave them hand-written. #730's split of `guides/tracks.md` by page (Sound, Effects,
   Mod, Gen) lines the guides up one-to-one with the page components. That's a docs change, with no
   key change.
5. **Not now.** Putting the overlays' keys in the table would add rows to `Space k`, which is a
   visible change and so a feature.

---

## 6. Testing

**Today.**
- 60 files in `tests/all/`, one binary since #719, drive keys into an `App` with a real engine.
- Screen readers live in `tests/all/common/mod.rs`: `screen` (:169), `track_page_text` (:306),
  `fx_page_text` (:333), `track_page_buffer` (:427) and `screen_cells` (:503).
- The SVG screenshots: `examples/screenshots.rs`, with `screenshots_are_the_same_on_every_run`
  (:872).
- The PNGs are pinned by CRC (`kitty/tests.rs`).
- Widget unit tests: `bar.rs` has 4. `grid.rs`, `setting.rs`, `device.rs` and `panel.rs` have none.

**What the design makes possible.**

| Level | How | Example |
|---|---|---|
| Widget | render into a `Buffer`, read symbols and colours, as `bar.rs:186-196` does | a `HexFine` cell focused on its fine part; a toggle at its default; typing `4_`; a jump label over the value; a fader's inverted bar; a macro range; a faded grid keeping the cursor |
| `param::edit` | pure functions, no `App` | `stepped(TIME, 95 ms, up, large)`; `typed(GROUP, "g3")`; a `List { wraps: false }` stops at the end; a phase wraps |
| Page | `page.key(cmd, n, &mut services, &modes)`, with `Services` built for a test (a song, `connect(Engine::new(..))` as `app_for` in `tests/all/common/mod.rs:33` does) | the track page's moves, as `TrackPage::key` is already driven with a `Track` alone |
| Pictures | `page.panels(&ctx)` gives `PanelWant`s, data with no terminal | the Mod page wants an LFO cycle in the focused shade for the cursor's slot |
| App | as now: keys into `App`, the screen read back | unchanged |
| Screenshots | as now; every PR states no diff | unchanged |

The migration-time parity test (§3.7) is the guard for identical rendering. It is deleted with the
old path.

---

## 7. Module layout

**Today.** `lib.rs` has 62 flat `pub mod`s, and 46 files add an `impl App` block, 47 blocks in 34
of those modules (counted on 2026-11-15, after #787; #718 item 15, #733). Because everything is
`pub`, the dead-code lint can't see anything.

**Target.** Folders by what a module is. Pages co-locate their state, keys, edits and drawing, as
gitui's `components/` and helix's `ui/` keep a component's events and rendering together [known].
Only widgets stay apart.

```text
src/
  main.rs  lib.rs  run.rs                 the main loop, draw_screen
  app.rs  app/                            App = services + modes + pages + overlays; key dispatch,
                                          the jump list (from tree.rs), commands, loops
  services/                               (the split's)
    mod.rs  ctx.rs                        Services, Ctx (today's page.rs)
    report.rs                             the engine's last report: live, jam, levels, meters
    automating.rs  heard.rs  meter.rs  scope.rs
    banking.rs  analysing.rs  loading.rs  autosave.rs  config.rs  places.rs  audio.rs
  pages/
    page.rs                               Page, Outcome
    track/      mod.rs (TrackPage, moves), device.rs, controls.rs, sound.rs, modulation.rs,
                fx.rs, generators.rs, edit.rs (editing+tuning), pick.rs, typing.rs,
                presets.rs (presetting), patches.rs (patching), macros.rs (M; macroing's page half),
                draw/ (views/track.rs, views/track/*)
    mixer/      mod.rs (mixer_page), draw.rs (views/mixer)
    project/    mod.rs (project_page), draw.rs (views/project)
    audio/      mod.rs (audio_page), draw.rs (views/audio)
    sample/     mod.rs (sample_page), settings.rs, placing.rs, points.rs, slicing.rs, marking.rs,
                auditioning.rs, sampling.rs, wave.rs (sample_wave), waveform.rs, draw.rs
    song/       mod.rs (song_view), draw.rs
    chain/      mod.rs (tree's chain keys), draw.rs
    phrase/     draw.rs (views/phrase), timeline.rs, folding.rs, accent_slide.rs, recording.rs,
                inspector.rs
  overlays/
    picker/     picker.rs + tree.rs (input/picker*), draw.rs, picking.rs, explorer.rs,
                exploring.rs, tagging.rs
    command/    line.rs (command_line), parse.rs (input/command), completion.rs, history.rs
    help/       which_key.rs + keys.rs (views/help), guides.rs, guide.rs
    shape/      mod.rs (shaping), draw.rs
    mapping/    mod.rs (macroing's gm half), draw.rs
    jump.rs     (jumping)
  screen/                                 what's always on screen: status.rs, system.rs, meter.rs
  widgets/                                (views/widgets) bar, grid, device, panel, table, setting,
                                          toggle, hint, group, role, meter, image, popup (new),
                param/  cell.rs, edit.rs, typed.rs, choices.rs         (new)
                theme.rs  glyphs.rs
  input/                                  keys.rs, bindings.rs, mode.rs, grammar.rs, wheel.rs
  graphics/                               (kitty) protocol.rs, slots.rs, panels.rs, plot.rs,
                                          switches.rs, generators.rs, encoding.rs, mod.rs
  roto/                                   roto.rs + devices.rs, edits.rs, layouts.rs, mix.rs
  offline/                                bench.rs, dsp_probe.rs, track_notes.rs, ableton.rs,
                                          probe.rs (probe-keys)
  screenshot/                             screenshot.rs, symbols.rs (the example's)
```

**Rules for the move.**
- **Moves only, by `git mv`, in one quiet PR** after the pages exist (#733). If it's too big to
  review, use one PR per folder.
- **Public surface.** `pub(crate)` everywhere except what `tests/`, `examples/` and `main.rs`
  use. One `pub use` block in `lib.rs` keeps the test-facing paths stable, so the tests change only
  their `use` lines. The dead-code lint comes back. Expect a handful of finds: #718 found
  `Columns::area_of` (`views/widgets/table.rs:101`) and `SamplePage::slice_starts`
  (`sample_page.rs:214`).
- **Where feature files go.** Those that are really `impl App` blocks (`banking`, `slicing`,
  `shaping`, `picking`…) move to the page, overlay or service they belong to, as #320 says. The
  layout above is where each lands.

---

## 8. Migration plan

**Preconditions, not part of this plan.**
- #719's vt-tui part, which is the current branch and must land before the split.
- The services split itself (#320, decided, in progress). It must be the only vt-tui PR open.
- Space Echo (#736) is open. It touches `roto/layouts.rs`, so the Roto lookups (E6/V3/G3) wait for it.

**Rules for every PR** (#319):
- One area per PR.
- Behaviour identical: `just screenshots` gives no diff, and `just ci` passes.
- A new abstraction only where several users already exist.
- The Architecture section's Interface or Graphics part is updated in the same PR, as
  CONTRIBUTING requires for "how pages are built" and "how images are drawn". This retires #722's
  stale `App` counts as it goes.
- No two of these PRs open at once on the same files. At most two in flight, in different areas,
  per the disk-hygiene note.

| # | PR | Size | Depends on | Area | Users that justify it | Subsumes |
|---|---|---|---|---|---|---|
| U1 | `widgets/popup.rs`: `popup()`, `centred()`, `right_of()`, `above()`; the 7 frame copies and 8 rects use them | S | split | `views/{picker,guide,help,shape,mapping,inspector}.rs` | 7 + 8 | (#321's leftovers) |
| U2 | Widget tests: pin `SettingsGrid`, `setting`, `Device`, `GraphicPanel` as drawn today | S | split | `views/widgets/` (tests only) | – | – |
| U3 | `CellLook`: `widgets::setting` takes a look, not `&Ctx`; pages build it | S | U2 | `views/widgets/setting.rs` and its 13 callers | 13 call sites | – |
| U4 | `ParamCell` + `CellSource::cell`: `TrackCells` replaces the 6 impls and the 5 title-control copies; `heard()` replaces the 3 live copies; temporary parity test | M | U3; uses today's `Entry`/`Param`/`Control` until the description lands | `views/track*.rs`, `views/widgets/grid.rs` | 6 + 5 + 3 | – |
| U5 | Pictures as data: `PanelWant`, `PanelIds`, `Shape::png`, `Graphics::panels`; the zoom caption moves to the page; split `kitty.rs` | M | split; independent of U1–U4 | `kitty*`, `track_page.rs`, `run.rs`, the screenshot example | 5 sync callers, 7 id fields | **#728** |
| U6 | `TypedValue` for `App.entry`, `ShapePanel.typing` and `MappingView.typing`; one hex commit | S | split | `track_page/typing.rs`, `shaping.rs`, `macroing.rs`, `app/entry.rs` | 3 | – |
| (M2) | *The device study's M2*: mod fields and track controls read `ModSpec` and the model accessors (M1's `ModSlot::set_field`, `Track::control`/`set_control`); `MACRO_LABELS` goes. Listed here for order only | M | split; M1, S1 | `track_page/`, `roto/` | 8 `ModField` dispatches, 4–6 `TrackControl` matches | **#726**'s vt-tui half |
| U7 = S2a | *The device study's S2, drawing half*: `fn view(control, track, song) -> Option<ParamView>` with the §3.1 gaps closed (`cell`, nudges each way, numbered lists, `set`); `TrackCells` builds each `ParamCell` from it, so the per-`Control` `value`/`mod_value` (`views/track.rs:107`, :210) go; the status line's `value_shown` (`views/status.rs:121`) reads `view.text` | M | U4; M2 | `views/track*.rs`, `views/status.rs`, `track_page/controls.rs` | every track-page cell, the status line | – |
| U8 = S2b | *The device study's S2, editing half*: `param::edit` (`stepped`, `stepped_fine`, `typed`, `reset`, `choices`), `choice_picker` and `set`. `adjust_control`, `commit_control`, `reset_control` and `open_control_picker` keep arms only for what `view` doesn't cover: `Sample`, `Tune`/`Cents`, `Mute`/`Solo` | M | U7; U6 | `track_page/{editing,pickers,typing}.rs` | one path for every described setting; most of the 42 `open_*_picker`s | completes **#726** |
| U9 | `Page` trait and `Page::KEYS`: `TrackPage` and `MixerPage` implement it; the key slices move beside them; a test pins the assembled table | S | split | `page.rs`, `input/bindings.rs`, `track_page.rs`, `mixer_page.rs` | 2 now, 8 by U10 | – |
| U10a–f | The remaining pages as components (#320), one each: project + audio together, sample, song, chain, phrase. Each moves its fields off `App`, implements `Page`, and draws its settings rows with `ParamCell::parts` where it has them | M each (phrase L: the 630-line `draw`) | U9; U4 for rows | each page's files | 6 pages | #320 |
| (E6/V3/G3) | *The device study's Roto PRs*: layout lookups by kind, `key()` and `resolve` from the specs. Here, add only: knob turns in `roto/edits.rs` go through `param::edit` and `set`, the same path as a key | S–M | U8; after #736 merges; not during the split | `roto/` | 3 lookups, the edits' mirror | **#731** |
| U13 | Module layout: `pages/`, `overlays/`, `services/`, `graphics/`, `offline/`, `widgets/`, `screen/`; `pub(crate)`; the dead-code lint back | M | all the above | all of vt-tui, moves only | 62 modules | **#733** |

**Alongside, not blocked by this plan.**
- **#730**, splitting `guides/tracks.md` by page: docs, on a quiet day. It pairs naturally with
  U10, guide for page.
- **#727's vt-tui half**, the `presetting`/`patching` save skeleton: S, after the split. It lands
  in `pages/track/` under U13.
- **#724**: `GenParam::grid` returning a `Grid`. Its vt-tui follow-up (`DeviceSpec::new` in
  `generators.rs`) belongs with U4, or just before it.
- **#720, #725, #729, #721, #723, #732**: model and engine work. Not UI. The device-description
  study covers #720 and #724.
- **#722**: each PR above updates the Architecture section it changes. The rest of #722 is docs.

**Order, with the parallel description work.**

```text
#719 (tui) ─► services split (#320) ─┬─► U1 ─► U2 ─► U3 ─► U4 ──┐
                                     ├─► U5 (graphics, own area) │
                                     ├─► U6 ─────────────────────┼──────► U8 = S2b ─► E6/V3/G3 (#731, after #736)
                                     ├─► M2 (#726) ──► U7 = S2a ◄┘          │
                                     └─► U9 ─► U10a…f ◄─── U4 (rows) ───────┴─► U13 (#733)
device study, model side (any time): P0a, G1, M1, S1 ─► M2;  E1, E2 after #736;  P0b around the split
```

The two plans share four PRs: M2, S2 (split here into U7 and U8) and E6/V3/G3. They're listed in
both plans so the order is visible from either, but each is one PR. The device study owns M2 and
E6/V3/G3. U7 and U8 are the device study's S2, written to the widget contract in §3.

---

## 9. Open points

1. **For the split.** Put the program's cursor and the folds in `Services`, not on `App`, so the
   mixer's `h`/`l`/`0`/`$` (`mixer_page.rs:139-150`) can be the page's own. Also decide whether
   `Modes` (view, mode, entry, field jump, count) is a struct of its own that keys read. The
   sketches assume yes.
2. **For the device study.** Its `ParamView` (§2.5 there) fits the widgets, with four gaps
   (§3.1 here). Without them the cells can't render identically:
   - **a cell text apart from the status line's**: hex `80` against `1.21 kHz`, and a generator's
     `brief` against `shown`;
   - **steps each way**: an envelope time steps down by `time_step(value - 1)`
     (`editing.rs:71-78`);
   - **whether a choice list is numbered**: `"{at:02X}  {name}"` (`pickers.rs:480`);
   - **a writer beside `view`**: `set(control, value, fine) -> Option<Action>`.

   Its `Typing` for every parameter, already its plan, gives the widgets one typing rule. Settling
   the two `Units` enums is its call.
3. **For the user.** The two inconsistencies in 1.5 (the mixer and `with_macros`; no mark on a
   macro-held cell without a bar) are left alone by every PR here. Should either be fixed later, as
   its own `fix:` or `feat:`?
4. **The overlays' keys** in `Space k` would be a visible addition. I left it out as a feature.
