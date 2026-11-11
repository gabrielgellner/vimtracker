# FX page redesign: devices, three rows and real graphics

**Status:** approved by the author on 2026-11-10: option A, with the effect's type in the title,
reached with `k` (question 1), and two-state settings as the bypass switch's glyph (question 2,
#375). It's milestone M28: step 1, the devices, the three-row grid and today's graphics in a framed
panel, is #374 and decision 139; the bypass switch is #375; bigger, smooth graphics are #376. The
study is kept as it was written, so its "now" is the page before #374.

A design study for the user's request of 2026-11-10: a bypass toggle, three rows of settings, a
bigger and smoother compressor graphic with a border, and a rethought FX page in Ableton's device
language. It assumes **four slots per track** (`INSERTS = 4`, in progress) and twelve settings a
slot (`MAX_PARAMS`, decision 135). No code is changed.

**Recommendation in one line:** option A, a rack of four full-width devices stacked down the page.
Each device has a title bar with a toggle, three spaced rows of four settings, and a framed graphic
on the right. Four devices of 9 rows fill 36 of the page's 38 rows, so the layout still fits inside
the Keys box.

---

## 1. What isn't working now

From `views/track.rs`, `track_page.rs`, `kitty.rs` and `track-fx.svg`:

* **It reads as a wall of text.** Each slot is 2 rows of 6 columns (`FX_COLUMNS = 6`). The columns
  use 5-cell bars and are sized by `FX_NAME_WIDTHS = [9, 10, 10, 6, 7, 10]`, the longest name each
  column has in *any* effect. So column 1 holds Comp's `Type`, EQ's `Low freq` and Delay's `Time`,
  and its width serves none of them. Only a blank row separates the slots, and the slot number and
  effect name sit in the first row's left margin. Nothing frames a slot as one thing.
* **The two-row pairing is a leftover.** Settings are laid out in pairs (`insert_row`: even settings
  on top, odd underneath), so the order of each effect's settings was chosen to make good pairs.
  With three rows those pairs break: EQ's bands would split across columns, for example. The
  layout needs to be stated per effect, not left to the order of the settings.
* **The curve is tiny and pixelated.** A compressor's curve is 4×2 cells, which is 60×78 screen
  pixels at 15×39. `line_png` draws it on 6×12 dots a cell, each dot a 3-pixel block, so the image
  is 72×72 pixels with 24×24 dots. Kitty stretches that square image into a cell area that isn't
  square. The line is one dot thick, stair-stepped and not anti-aliased. The EQ's 10×2 cells have
  the same problem.
* **The graphics have no frame.** The curve floats after the last column with no box, no grid and no
  threshold mark, so you can't tell where 0 dB or the knee is. The Sound page boxes its filter
  curve (`image::boxed`), but the FX page doesn't.
* **The GR meter is coarse.** It's 1 cell by 2 rows over 24 dB, so each eighth of a cell is 1.5 dB.
  The pumping is visible but blunt.
* **Bypass is barely visible.** A bypassed slot shows only as faint text. Its curve stays at full
  brightness, because fading it needs another image (decision 59).

## 2. Layout options

How to read the mockups:

* Every block is exactly 163×42 and shows the whole screen: the 150-column view, the 13-column main
  meter, and the two status rows.
* The fixed-width font can't show colour or weight, so the conventions are:
  * `›` marks the cursor. On screen it's the Rosewater block, as now.
  * Braille stands in for the kitty images. On screen they're smooth, anti-aliased lines at full
  pixel resolution (§4).
  * `` is `nf-fa-toggle_on` (U+F205) and `` is `nf-fa-toggle_off` (U+F204). Your terminal may
    hide them if its font lacks Nerd Font symbols.
* The chain is Glue → EQ3 → Distort → Delay.
* **Slot 1 has the focus:** its frame is drawn in `OVERLAY2`, its effect name is bold and its curve
  is green. The other frames are drawn in `SURFACE1`.
* **Slot 3 is bypassed:** its toggle is the faint `` and everything inside its frame is `OVERLAY0`.

### Option A: a stacked rack (recommended)

Each slot is a full-width device 9 rows tall:

* **Frame and title.** A rounded frame, as every window has (decision 103). The title sits on the
  top border.
* **Settings.** Three rows of up to four settings. Between the rows is a blank line, which gives
  the device room to breathe and matches the graphic's height.
* **Graphic.** A framed graphic panel aligned to the right edge.

Four devices, with no gap rows between them, take rows 2 to 37. The frames' own borders separate
them, and every device keeps its position whatever the slot holds.

```
 Track 01 Drums   Sound  Mod  FX                                                                                            CPU  4%  late 0  xruns 0  ⢀⡤⣄   ⢀⡀    
                                                                                                                                                       ⠼ ⠘⣆⢀⡴⠋⠙⢦⣀⡴⠚
╭─ 1    Comp  Glue ─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮    ⠈⠉       
│                                                                                                                         ┌─────────────┐  ┌──┐      │             
│  Threshold›40 ▕██▎      ▏   Attack    0.3 ms           Makeup    30 ▕█▊       ▏   Mix       FF ▕█████████▏              │             │  │██│   0  │     Main    
│                                                                                                                         │        ⣀⣀⣠⠤⠤│  │▀▀│      │    -9 -8    
│  Ratio     4:1              Release   0.4 s            Auto       Off            Peak Clip  On                        │     ⣀⡴⠋⠁    │  │  │  -8  │  +6       ▔▔
│                                                                                                                         │  ⢀⡤⠞⠁       │  │  │      │             
│  Range     C0 ▕██████▊  ▏   Source    Self                                                                              │⣠⠖⠋          │  │  │ -16  │             
│                                                                                                                         └─────────────┘  └GR┘      │   0         
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯             
╭─ 2    EQ3 ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮             
│                                                                                                               ┌─────────────────────────────────┐  │  -6         
│  Low freq  3B ▕██▏      ▏   Mid freq  90 ▕█████▏   ▏   High freq DD ▕███████▊ ▏   Gain      E0 ▕███████▉ ▏    │                                 │  │             
│                                                                                                               │⣀⣀⡀                              │  │             
│  Low       A0 ▕█████▋   ▏   Mid       50 ▕██▉      ▏   High      98 ▕█████▍   ▏                               │  ⠉⠉⠉⠉⠉⠉⠉⠓⠒⠒⠲⠤⢤⣀  ⢀⣠⠤⠤⠖⠒⠒⠒⠒⠒⠚⠉⠉⠉⠉│  │             
│                                                                                                               │               ⠈⠉⠉⠉              │  │ -12         
│                             Mid Q     40 ▕██▎      ▏                                                          │                                 │  │             
│                                                                                                               └─────────────────────────────────┘  │     ▅▅ ▃▃   
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯     ██ ██   
╭─ 3    Distort  Overdrive ─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮ -18 ██ ██   
│                                                                                                                                                    │     ██ ██   
│  Amount    90 ▕█████▏   ▏   Mix       FF ▕█████████▏                                                                                               │     ██ ██   
│                                                                                                                                                    │     ██ ██   
│  Tone      60 ▕███▍     ▏   Gain      C8 ▕███████  ▏                                                                                               │ -24 ██ ██   
│                                                                                                                                                    │     ██ ██   
│                                                                                                                                                    │     ██ ██   
│                                                                                                                                                    │     ██ ██   
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯     ██ ██   
╭─ 4    Delay ──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮ -36 ██ ██   
│                                                                                                                                                    │     ██ ██   
│  Time      3/16             Low cut   20 ▕█▏       ▏   Width     FF ▕█████████▏                                                                    │     ██ ██   
│                                                                                                                                                    │     ██ ██   
│  Feedback  70 ▕████     ▏   High cut  B0 ▕██████▎  ▏   Mix       50 ▕██▉      ▏                                                                    │ -48 ██ ██   
│                                                                                                                                                    │     ██ ██   
│  Ping-pong  On             Drive     18 ▕▉        ▏   Gain      E0 ▕███████▉ ▏                                                                    │     ██ ██   
│                                                                                                                                                    │     ██ ██   
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯ -60 ██ ██   
                                                                                                                                                           L  R  G 
                                                                                                                                                                   
 TRACK  Effect  󰐊  120 BPM  Groove Straight  Oct 4 Vel 64  TVol E0 *                                                                                               
                                                                                                                                                                   
```

The same layout with other states:

* **Slot 1, no graphic.** Chorus has no graphic, so its device is simply settings and space.
* **Slots 2 and 3.** A Clean comp with a sidechain `Source`, and a limiter, which shows the GR meter
  alone.
* **Slot 4, empty and focused.** Its frame stays full size, so nothing moves when an effect goes in.
  It reads as an empty bay.

```
󰙽 Track 02 Bass   Sound  Mod  FX                                                                                             CPU  4%  late 0  xruns 0  ⢀⡤⣄   ⢀⡀    
                                                                                                                                                       ⠼ ⠘⣆⢀⡴⠋⠙⢦⣀⡴⠚
╭─ 1    Chorus  Ensemble ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮    ⠈⠉       
│                                                                                                                                                    │             
│  Rate      28 ▕█▍       ▏   Mix       80 ▕████▌    ▏                                                                                               │     Main    
│                                                                                                                                                    │    -9 -8    
│  Depth     70 ▕████     ▏   Gain      E0 ▕███████▉ ▏                                                                                               │  +6       ▔▔
│                                                                                                                                                    │             
│                                                                                                                                                    │             
│                                                                                                                                                    │   0         
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯             
╭─ 2    Comp  Clean ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮             
│                                                                                                                         ┌─────────────┐  ┌──┐      │  -6         
│  Threshold 58 ▕███▏     ▏   Attack    30 ▕█▊       ▏   Makeup    20 ▕█▏       ▏   Mix       FF ▕█████████▏              │             │  │▀▀│   0  │             
│                                                                                                                         │          ⣀⣠⠤│  │  │      │             
│  Ratio     60 ▕███▍     ▏   Release   80 ▕████▌    ▏   Auto       On                                                   │     ⣀⡴⠒⠋⠉⠁  │  │  │  -8  │             
│                                                                                                                         │  ⢀⡤⠞⠁       │  │  │      │ -12         
│                             Source    01                                                                                │⣠⠖⠋          │  │  │ -16  │             
│                                                                                                                         └─────────────┘  └GR┘      │     ▅▅ ▃▃   
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯     ██ ██   
╭─ 3    Limit ──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮ -18 ██ ██   
│                                                                                                                                          ┌──┐      │     ██ ██   
│  Input     30 ▕█▊       ▏   Mix       FF ▕█████████▏                                                                                     │▔▔│   0  │     ██ ██   
│                                                                                                                                          │  │      │     ██ ██   
│  Ceiling   FC ▕████████▉▏   Gain      E0 ▕███████▉ ▏                                                                                     │  │  -8  │ -24 ██ ██   
│                                                                                                                                          │  │      │     ██ ██   
│  Release   AA ▕██████   ▏                                                                                                                │  │ -16  │     ██ ██   
│                                                                                                                                          └GR┘      │     ██ ██   
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯     ██ ██   
╭─ 4    ›Empty ──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮ -36 ██ ██   
│                                                                                                                                                    │     ██ ██   
│                                                                                                                                                    │     ██ ██   
│                                                                                                                                                    │     ██ ██   
│  K  pick an effect                                                                                                                                 │ -48 ██ ██   
│                                                                                                                                                    │     ██ ██   
│                                                                                                                                                    │     ██ ██   
│                                                                                                                                                    │     ██ ██   
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯ -60 ██ ██   
                                                                                                                                                           L  R  G 
                                                                                                                                                                   
 TRACK  Effect  󰐊  120 BPM  Groove Straight  Oct 4 Vel 64  TVol E0 *                                                                                               
K picks an effect for slot 4                                                                                                                                       
```

**Why the empty space works:** a device with no graphic still has a full frame and title. The space
on the right reads as "nothing to draw", like a short line of text. It doesn't look like a missing
picture, because nothing marks where a picture would go. The graphic panels are right-aligned, so
the curves that do exist line up down the page.

### Option B: Ableton's horizontal chain

* **Devices.** Four devices side by side in signal-flow order, each 36 columns wide, with the
  graphic on top. Below it is a compact grid of 4 columns by 3 rows, each setting a short label over
  its value, as on Elektron screens.
* **What the width forces.** At 36 columns, a 4-column grid needs 4-letter labels (`THRS`, `RTIO`),
  values cut to 7 characters, and 3-cell bars.
* **What the height leaves.** A device with no graphic is shorter, because a framed but empty
  graphic area looks broken. Its bottom is then ragged against its neighbours.

```
 Track 01 Drums   Sound  Mod  FX                                                                                            CPU  4%  late 0  xruns 0  ⢀⡤⣄   ⢀⡀    
                                                                                                                                                       ⠼ ⠘⣆⢀⡴⠋⠙⢦⣀⡴⠚
╭─ 1    Comp  Glue ───────────────╮  ╭─ 2    EQ3 ──────────────────────╮  ╭─ 3    Distort  Overdrive ───────╮  ╭─ 4    Delay ────────────────────╮    ⠈⠉       
│ ┌─────────────────────┐ ┌──┐     │  │ ┌──────────────────────────────┐ │  │                                  │  │                                  │             
│ │                     │ │██│  0  │  │ │                              │ │  │ AMNT    MIX                      │  │ TIME    LCUT    WDTH             │     Main    
│ │                     │ │██│     │  │ │                              │ │  │ 90▕█▊ ▏ FF▕███▏                  │  │ 3/16    20▕▍  ▏ FF▕███▏          │    -9 -8    
│ │               ⣀⣀⣠⠤⠤⠖│ │▀▀│ -6  │  │ │                              │ │  │                                  │  │                                  │  +6       ▔▔
│ │          ⢀⡤⠖⠋⠉⠁     │ │  │     │  │ │⠉⠉⠉⠉⠓⠒⠒⠦⠤⢤⣀⣀        ⣀⣀⣀⣠⠤⠤⠤⠖⠒⠒│ │  │ TONE    GAIN                     │  │ FDBK    HCUT    MIX              │             
│ │        ⣠⠖⠋          │ │  │-12  │  │ │           ⠈⠙⠲⣄⡀⣀⡴⠚⠉⠁         │ │  │ 60▕█▏ ▏ C8▕██▍▏                  │  │ 70▕█▍ ▏ B0▕██▏▏ 50▕█  ▏          │             
│ │     ⣀⡴⠋⠁            │ │  │     │  │ │               ⠉⠁             │ │  │                                  │  │                                  │   0         
│ │  ⢀⡤⠞⠁               │ │  │-18  │  │ │                              │ │  │                                  │  │ PING    DRIV    GAIN             │             
│ │⣠⠖⠋                  │ │  │     │  │ │                              │ │  │                                  │  │  On    18▕▎  ▏ E0▕██▋▏          │             
│ └─────────────────────┘ └GR┘     │  │ └──────────────────────────────┘ │  ╰──────────────────────────────────╯  ╰──────────────────────────────────╯  -6         
│                                  │  │                                  │                                                                                         
│ ›THRS   ATK     MKUP    MIX      │  │ LFRQ    MFRQ    HFRQ    GAIN     │                                                                                         
│ 40▕▊  ▏ 0.3 ms  30▕▋  ▏ FF▕███▏  │  │ 3B▕▊  ▏ 90▕█▊ ▏ DD▕██▋▏ E0▕██▋▏  │                                                                                         
│                                  │  │                                  │                                                                             -12         
│ RTIO    REL     AUTO    PCLP     │  │ LOW     MID     HIGH             │                                                                                         
│ 4:1     0.4 s    Off    On     │  │ A0▕█▉ ▏ 50▕█  ▏ 98▕█▊ ▏          │                                                                                 ▅▅ ▃▃   
│                                  │  │                                  │                                                                                 ██ ██   
│ RNGE    SRC                      │  │         MIDQ                     │                                                                             -18 ██ ██   
│ C0▕██▎▏ Self                     │  │         40▕▊  ▏                  │                                                                                 ██ ██   
╰──────────────────────────────────╯  ╰──────────────────────────────────╯                                                                                 ██ ██   
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
                                                                                                                                                                   
 TRACK  Effect  󰐊  120 BPM  Groove Straight  Oct 4 Vel 64  TVol E0 *                                                                                               
                                                                                                                                                                   
```

**The trade-offs:**

* **For:** it's the closest to Ableton's look. Left to right *is* the signal flow, and `l` past a
  device's last column could walk into the next device.
* **Against:**
  * 15 rows go unused.
  * The bars are too short to read a value well.
  * Every setting needs a new 4-letter display name. The lock labels can't always serve:
    `GRTO`, `CATK`, `PTCH` and the like are prefixed for the step grid.
  * The ragged bottoms.
  * Five or more slots don't fit across.

### Option C: a chain strip plus one big device

* **The strip.** A row of four compact tiles, one per slot, each showing its toggle and three
  headline values. A comp's tile also shows a live GR bar.
* **The device.** Under the strip, the focused slot as one large device: a 36×14-cell curve, a
  14-row GR meter, and a **GR history**, a text sweep of the last two bars of reduction that shows
  the pumping over time.
* **Moving between slots.** `{` and `}` change which slot is big.

```
 Track 01 Drums   Sound  Mod  FX                                                                                            CPU  4%  late 0  xruns 0  ⢀⡤⣄   ⢀⡀    
                                                                                                                                                       ⠼ ⠘⣆⢀⡴⠋⠙⢦⣀⡴⠚
╭─ 1    Comp  Glue ───────────────╮  ╭─ 2    EQ3 ──────────────────────╮  ╭─ 3    Distort  Overdrive ───────╮  ╭─ 4    Delay ────────────────────╮    ⠈⠉       
│ THRS 40  RTIO 4:1   GR ██▌       │  │ LFRQ 3B  MFRQ 90  HFRQ DD        │  │ AMNT 90  TONE 60  MIX FF         │  │ TIME 3/16  FDBK 70  MIX 50       │             
╰──────────────────────────────────╯  ╰──────────────────────────────────╯  ╰──────────────────────────────────╯  ╰──────────────────────────────────╯     Main    
                                                                                                                                                          -9 -8    
╭─ 1    Comp  Glue ─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮  +6       ▔▔
│ ┌────────────────────────────────────┐  ┌──┐      ┌──────────────────────────────────────────────────────────────────────────────────────────────┐ │             
│ │                                    │  │██│   0  │█████████▀▀▀▀▔▔▔████████▀▀▀▔▔▔▔▔████████▀▀▀▀▔▔▔▔█████████▀▀▀▀▔▔▔█ ██████▀▀▀▔▔▔▔▔████████▀▀▀▀▀▔│ │             
│ │                                    │  │██│      │█████▀▀▔        █████▀▔▔        ████▀▔▔         █████▀▀▔        █ ██▀▀▔         ████▀▀▔▔      │ │   0         
│ │                                    │  │▀▀│      │███▀▔           ███▀▔           ██▀             ███▀▔           █ █▔            ██▀▔          │ │             
│ │                                    │  │  │  -6  │█▀▔             ██▔             ▀               ██▔             █               ▀▔            │ │             
│ │                            ⣀⣀⣠⠤⠤⠖⠒⠒│  │  │      │▀               █▔                              ▀▔              ▀                             │ │  -6         
│ │                    ⢀⣠⠤⠖⠒⠚⠉⠉⠁       │  │  │      │                                                                                              │ │             
│ │                  ⣠⠴⠋               │  │  │ -12  │                                                                                              │ │             
│ │               ⢀⡴⠚⠁                 │  │  │      │                                                                                              │ │             
│ │             ⣠⠖⠋                    │  │  │      │                                                                                              │ │ -12         
│ │          ⢀⡴⠋⠁                      │  │  │ -18  │                                                                                              │ │             
│ │       ⢀⣠⠞⠉                         │  │  │      │                                                                                              │ │     ▅▅ ▃▃   
│ │     ⣠⠴⠋                            │  │  │      │                                                                                              │ │     ██ ██   
│ │  ⢀⡤⠞⠁                              │  │  │ -24  │                                                                                              │ │ -18 ██ ██   
│ │⣠⠖⠋                                 │  │  │      │                                                                                              │ │     ██ ██   
│ └────────────────────────────────────┘  └GR┘      └─ GR over the last two bars ──────────────────────────────────────────────────────────────────┘ │     ██ ██   
│                                                                                                                                                    │     ██ ██   
│                                                                                                                                                    │ -24 ██ ██   
│  Threshold›40 ▕████▎            ▏    Attack    0.3 ms                    Makeup    30 ▕███▎             ▏    Mix       FF ▕█████████████████▏      │     ██ ██   
│                                                                                                                                                    │     ██ ██   
│  Ratio     4:1                       Release   0.4 s                     Auto       Off                     Peak Clip  On                        │     ██ ██   
│                                                                                                                                                    │     ██ ██   
│  Range     C0 ▕████████████▊    ▏    Source    Self                                                                                                │ -36 ██ ██   
│                                                                                                                                                    │     ██ ██   
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯     ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                       -48 ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                           ██ ██   
                                                                                                                                                       -60 ██ ██   
                                                                                                                                                           L  R  G 
                                                                                                                                                                   
 TRACK  Effect  󰐊  120 BPM  Groove Straight  Oct 4 Vel 64  TVol E0 *                                                                                               
                                                                                                                                                                   
```

**The trade-offs:**

* **For:** the biggest, most beautiful graphics, and the GR history shows pumping better than any
  meter. It's the way M8 and Elektron work: one effect per screen. It also scales to any number of
  slots, because the strip just scrolls.
* **Against:** you can see only one slot's settings at a time. That breaks the tracker habit of
  reading the whole chain at a glance, and it means `{` and `}` to compare two slots' values.

**Suggested use:** take C's big device as a *later zoom* on top of A, such as a key that toggles the
focused device to full page. Don't adopt it as the page itself.

## 3. Device anatomy (option A)

```
╭─ 1    Comp  Glue ───────────────── … ──╮      frame: rounded, SURFACE1; OVERLAY2 when focused
   │  │  │     └ type: a control, K picks it, value colour
   │  │  └ effect: a control, K picks it, bold when focused
   │  └ bypass:  nf-fa-toggle_on U+F205 GREEN  /   nf-fa-toggle_off U+F204 faint
   └ slot digit, dim

┌─────────────┐  ┌──┐
│             │  │██│   0   GR: text, 2×5 cells, 0 to −20 dB, 0.5 dB an eighth,
│ curve image │  │▀▀│       hanging from the top; a faint ▔ holds the deepest
│  13×5 cells │  │  │  -8   reduction of the last second
│ 195×195 px  │  │  │
│             │  │  │ -16
└─────────────┘  └GR┘
 faint square box (image::boxed), right-aligned, inner rows 0–6 of the device
```

### Title bar

`╭─ 1  ⟨toggle⟩  Comp  Glue ───…───╮`. It's drawn on the top border, as the Keys box writes `Keys`
on its own.

* **Slot digit:** `dim`.
* **Bypass toggle:** it reuses `glyphs::SWITCH_ON` (`nf-fa-toggle_on`, U+F205) in `GREEN` while the
  slot is in, and `glyphs::SWITCH_OFF` (`nf-fa-toggle_off`, U+F204) in `faint` while it's bypassed.
  * **Why these glyphs:** they're the switch the explorer already uses (decision 101), so the same
  idea gets the same glyph.
  * **Rejected:** `nf-md-toggle_switch` / `toggle_switch_off` (U+F0521 / U+F0522), which would add a
  second toggle style, and `nf-md-power` (U+F0425), Ableton's activator, because the user asked for
  a toggle.
  * **While playing,** the toggle shows what's *heard*, `heard_bypassed`, so a `nBYP` lock flips it
    with its step (decision 88).
  * **Not a cursor stop:** `b` works from anywhere in the device.
* **Effect name:** a control (`Control::Insert(slot)`), in the value colour and bold when the slot
  has the focus. `K` on it picks an effect.
* **Type:** a control too. The type is the setting that changes an effect's character: `Type` for
  Comp, Distort and Chorus, and `Mode` for LPG and Comb. Moving it to the title takes it out of the
  grid, which frees a cell; Glue's 11 settings become 10 in the grid. `K` on the type picks one.
  Effects without a type show nothing after the name.

### Settings: three rows of four

* **Cells.**
  * Every cell is `name 10 │ value 3 │ bar ▕9▏ │ gap 3`, which is 27 columns. Four columns take
    105 columns.
  * **Value bars** are 9 cells, the Sound page's `BAR_CELLS`, so a bar on the FX page now looks
    exactly like one on the Sound page. They're `ValueBar` with `.playing(live)`, as now, so
    modulated values still show what's playing.
  * **A list setting** spans the value and the bar, 14 columns.
  * **An `Off`/`On` setting** (Auto, Peak Clip, Ping-pong, Freeze) draws the switch glyph before
    its word: ` On` or ` Off`. This is optional (question 2).
* **The grid is stated per effect, not derived from the order of the settings.** Add
  `Insert::grid() -> [[Option<usize>; 3]; 4]` in `vt-model`, next to `params()`.
  * **Default:** the effect's own settings column by column in their order, then an output column
    after them (`Mix`, `Gain`, and `Width` or `Makeup` where they exist).
  * **Hand-placed grids** for the few where meaning beats order:

| Effect | Column 1 | Column 2 | Column 3 | Column 4 |
|---|---|---|---|---|
| Glue | Threshold, Ratio, Range | Attack, Release, Source | Makeup, Auto | Mix, Peak Clip |
| Clean / Console | Threshold, Ratio | Attack, Release, Source | Makeup, Auto | Mix |
| EQ3 (columns are bands) | Low freq, Low | Mid freq, Mid, Mid Q | High freq, High | Gain |
| Delay | Time, Feedback, Ping-pong | Low cut, High cut, Drive | Width, Mix, Gain | |
| Distort (pedals) | Amount, Tone | Mix, Gain | | |

  * **Capacity:** at most 9 settings of the effect's own plus 3 output settings, which is 12, so
    `MAX_PARAMS` still fits exactly.
  * **Gaps:** a grid can leave holes, such as Glue's third row in columns 3 and 4. The cursor skips
    them (§5).

### The graphic panel

* **Placement:** aligned right with 2 columns of padding. It spans the device's inner rows 0 to 6:
  a faint square box (`image::boxed`) with the image in rows 1 to 5.
* **Comp:** a curve box of 15×7 (the image is 13×5 cells, which is **195×195 px** at 15×39, square)
  and a 2-column gap. Then a GR meter box of 4×7 with `GR` written into its bottom border, and a
  scale of `0`, `-8` and `-16` on its right. That's 25 columns in all.
* **EQ3:** a box of 35×7 (the image is 33×5 cells, **495×195 px**).
* **Limit:** the GR meter alone.
* **Everything else:** no panel for now (§4).

**Size against today:**

| | now | option A |
|---|---|---|
| Comp curve | 4×2 cells, 24×24 dots → 72×72 px stretched to 60×78 | 13×5 cells, drawn at 195×195 real pixels |
| GR meter | 1×2 cells, 24 dB, 1.5 dB per eighth | 2×5 cells, 20 dB, **0.5 dB per eighth**, with a scale |
| EQ curve | 10×2 cells | 33×5 cells, 495×195 px |

The comp curve covers about 8 times the screen area it does now, and its detail is finer still,
because it's drawn at real pixels instead of 3-pixel blocks.

### Borders and focus

* **Device frames:** `widgets::window_border()` (rounded).
* **Graphics:** square, faint boxes, as the Sound page's curve and waveform have, because they're
  part of the grid rather than windows.
* **Focus is brightness, not colour.**
  * **The focused device:** frame in `OVERLAY2`, effect name bold, curve in the active green (as
    now).
  * **The other devices:** frames in `SURFACE1`, curves in `dim`.
* **Not used:** heavy or double lines (they look clunky in a terminal), filled title bars, and
  colour for focus. The Keys box keeps yellow as the only coloured frame.

## 4. Graphics

### Comp curve: bigger and smooth

* **Drawn at the terminal's pixels.** Size the picture to the cell size `Graphics::cell` already
  tracks for the sample page, e.g. 195×195 px. Each cell then shows exactly its own pixels and
  nothing is stretched. A font-size change already re-sends images through the `use_window` path.
* **Anti-aliasing.**
  * **Format:** keep `kitty::Picture` (a palette PNG with 4 bits a pixel), but give the palette
    alpha. Today `tRNS` makes only entry 0 see-through; give it an alpha per entry. Entries 1 to 8
    are then the line colour at alpha 1/8 to 8/8, which leaves room for two grid shades and a mark.
  * **Drawing:** rasterise the polyline with coverage: for each pixel, its distance to the nearest
    segment gives `alpha = clamp(w/2 + 0.5 − d)` for a line `w` = 2 px wide, rounded to eighths.
    That's tens of lines in a new `kitty::draw` (or `views/curve.rs`) helper, which the EQ and later
    graphics share.
* **Inside the image, all faint (`OVERLAY0` at 40 to 60%):**
  * dotted grid lines at −20 and −40 dB on both axes (0 dB is the box's top and right edges);
  * a dotted 1:1 diagonal, so how far the curve bends away from it *is* the compression;
  * **the threshold:** a dashed vertical line at the threshold's input level from the bottom edge
    up to the curve. The knee itself is in the curve colour, as part of the line.
* **No text labels on the curve.** The numbers belong to the settings, and the GR meter has its own
  scale.
* **Three colours, three cached images:** dim, active (focused) and faint (bypassed). The faint one
  is new and fixes "a bypassed slot's curve stays bright". Each is sent once and then cached in
  `Graphics::slots`, as the two existing variants are.

### GR meter: keep the pumping

* **Text, not an image,** so it moves at every report for free, as now (`hanging_cell`). It's 2
  cells wide and 5 rows tall, hangs from the top, and covers 0 to −20 dB, so each eighth is 0.5 dB:
  three times finer than now, which makes the pumping read as motion rather than steps.
* **A held mark:** add a faint `▔` at the deepest reduction of the last ~1 s, as the main meter
  holds its top. You then see both the pump and how deep it goes, all in text.
* **Colour:** unchanged from today (`dim`, or active while focused).
* **Later, a ball on the curve.** A 5-px dot placed on the curve at the current input level, moved
  every frame by changing its placement's pixel offset. It's the sample page's mark technique
  (decision 109: one stored image, about 56 bytes a move, stored again after a clear).
  * **Data:** none new. Above the threshold the static curve is monotonic, so the input level can
    be worked out from the GR the engine already reports. At zero GR the dot parks at the knee.
  * **Prior art:** Ableton's Compressor shows this dot.

### EQ curve at the new size

* **Size:** 33×5 cells, 495×195 px, drawn at real pixels with the same line drawing.
* **Faint grid:** 0 dB as a dotted line, ±12 dB fainter, and vertical lines at 100 Hz, 1 kHz and
  10 kHz.
* **Range:** ±18 dB, as now (decision 60).
* **Later:** a dot at each band's frequency and gain, brighter for the band the cursor is in. That
  costs an image for each band change of focus, so it's left out for now.

### Image cost (rules unchanged)

* **When images are sent:** only when what they show changes (`SlotDrawing` compared as now) or the
  focus/bypass state changes. Motion stays text (the GR meter and the held mark) or placement moves
  (the ball, later).
* **Sizes, at 4 bits a pixel:** the comp curve is about 19 KB stored, and the EQ about 48 KB.
  `Picture::png` compresses only past one stored block (64 KB), so lower that threshold for these,
  or always compress: a mostly empty picture deflates to about 2 to 3 KB (decision 93's measures).
* **Throttling:** a curve that follows a modulated threshold, or an LFO on an EQ band, would send a
  new image every frame. Throttle slot redraws to every other frame, as the scope does
  (`SCOPE_FRAMES`, #214).

### Graphics for later

All of these are static images sent on change, worked out from each effect's own maths, as the comp
and EQ curves are:

* **Distort:** the transfer curve (Soft, Hard, Fold, Sine, pedals), drawn like the comp curve.
* **Delay:** the taps as decaying impulses, with ping-pong alternating L and R.
* **Reverb / Plate:** the decay envelope, with the pre-delay and damping.
* **Filter-like effects** (LPG, Reso, Comb, Vowel, Phaser at rest): a frequency response, as the EQ
  draws.
* **Limit:** the ceiling as a line over the GR meter.
* **GR history** (option C's sweep), as text in the panel's space for a comp.

## 5. Navigation

* **Rows.** A slot is 4 cursor rows: the title (Effect, Type), then settings rows 1 to 3.
  * `SLOT_ROWS` goes from 2 to 4 and `FX_COLUMNS` from 6 to 4.
  * `FX_ROWS` stays a `const`, made of *cells*: `Control::InsertParam(slot, cell)`, where `cell`
    goes through `Insert::grid()` to reach the setting's index.
* **`h` / `l`** move along a row and skip holes. `0` and `$` go to a row's ends. `h` in the first
  column no longer reaches the effect, because the effect is now *above*: `k` from row 1.
* **`j` / `k`** move down and up through the rows: title, 1, 2, 3, then the next slot's title, and
  so on. They keep the wanted column (`track_wanted`). A hole takes the nearest cell in that row.
  The title's `Effect` lines up with column 1 and `Type` with column 2.
* **`{` / `}`** go to the same row and column in the previous or next slot, with a count, as now.
  With `INSERTS = 4`, `}` stops at slot 4.
* **`gg` / `G`:** the first and last rows.
* **`K`** picks whatever is under the cursor: the effect, the type or a setting's value. It's
  unchanged per control.
* **`b`** bypasses the cursor's device from any of its rows. The toggle flips in the title.
* **`dd` / `p`** take the cursor's effect out and put it into the cursor's slot, from any row. They
  work as now.
* **Jump labels** (`FieldJump`) label the cells, including the title's two.

**Changes users will notice:** the effect moves from the far left to the title row, and the order of
settings changes to the per-effect grids.

## 6. Growing past four slots

Option A doesn't depend on there being exactly four.

* **Device size stays fixed.** Every device is 9 rows, whatever it holds.
* **Scrolling.** Past what fits (four at 38 rows, or four in the Keys box's 36), the page scrolls by
  whole devices to keep the cursor's device fully in view, as the phrase follows its cursor.
* **A chain line.** Row 1, blank today, becomes a **chain line** once a track has more slots than
  fit. It lists every slot in signal-flow order with its toggle. Slots in view are in text colour,
  the rest dim, the cursor's bold, and a marker shows how many are above or below. The top of the
  page, scrolled to slot 3 of 6 with the cursor on Plate:

```
 Track 01 Drums   Sound  Mod  FX                                                                                            CPU  4%  late 0  xruns 0  ⢀⡤⣄   ⢀⡀    
 1  Glue  ›  2  EQ3  ›  3  Distort  ›  4  Delay  ›  5  Plate  ›  6  Limit        ▲ 2 above                                                       ⠼ ⠘⣆⢀⡴⠋⠙⢦⣀⡴⠚
╭─ 3    Distort  Overdrive ─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮    ⠈⠉       
│                                                                                                                                                    │             
│  Amount    90 ▕█████▏   ▏   Mix       FF ▕█████████▏                                                                                               │     Main    
│                                                                                                                                                    │    -9 -8    
│  Tone      60 ▕███▍     ▏   Gain      C8 ▕███████  ▏                                                                                               │  +6       ▔▔
│                                                                                                                                                    │             
│                                                                                                                                                    │             
│                                                                                                                                                    │   0         
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯             
╭─ 4    Delay ──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮             
```

* **Not chosen:** shrinking devices so that more fit. The graphics would lose their size, which is
  the point of this design.
* **More LFOs and envelopes.** The device frame (`widgets::device`) is generic: a title with an
  optional toggle, a grid, and an optional panel. The Mod page could later show each LFO or
  envelope as a device, with the shape image in the panel, and scroll the same way when there are
  three LFOs and three envelopes. That's out of scope here.

## 7. Recommendation and cost

**Option A.**

* **It answers every point the user raised:** the toggle, the three rows, a framed and much bigger
  smooth curve, the pumping kept and refined, and structure in Ableton's spirit (framed devices
  with a title bar and a graphic where one helps).
* **It keeps what the tracker relies on:** all four slots and every setting are on one screen, and
  the columns line up for `j`, `k`, `{` and `}`.
* **It's consistent and it fits:** it uses the app's existing parts (rounded windows, faint image
  boxes, the Sound page's 9-cell `ValueBar`, the explorer's switch glyph), and fits 4×9 rows
  exactly, even in Keys mode.
* **Against the others:** B buys Ableton's look at the cost of readability. C is the best zoom, and
  can come later.

**Files that change:**

* `crates/vt-model/src/insert.rs`: `Insert::grid()` and the default grid, a `type_param()`, and
  tests that every effect's grid holds each of its settings exactly once, apart from the type.
* `crates/vt-tui/src/track_page.rs`:
  * `SLOT_ROWS = 4` and `FX_COLUMNS = 4`;
  * `FX_ROWS` as cells, plus the title row;
  * `fx_slot`, `row_start`, `fields_in`, and skipping holes;
  * `{` `}`, `b`, `dd` and `p` from any row;
  * the cursor tests.
* `crates/vt-tui/src/views/track.rs`: the FX branch is replaced by device drawing. `FX_NAME_WIDTHS`,
  `fx_columns`, `fx_row`, `slot_drawing` and `FX_DRAWING_COLUMN` go.
* New `crates/vt-tui/src/views/widgets/device.rs`: the frame, the title with its toggle, focus and
  bypass styles, and the right-aligned panel.
* `crates/vt-tui/src/kitty.rs`:
  * `SlotDrawing` sizes, and drawing at cell pixels;
  * `Picture` with alpha in its palette, and the anti-aliased line;
  * the grid and threshold;
  * a third colour state for bypassed;
  * the compression threshold and throttling.
* `crates/vt-tui/src/views/widgets/meter.rs`: no change; the reduction meter becomes 5 rows over
  20 dB in `track.rs`, with the held mark.
* `crates/vt-tui/src/screenshot.rs` and `just screenshots`: `track-fx.svg`. Docs: the manual's FX
  page, `vimtracker.md`'s visual style and architecture sections, and new decisions in
  `decisions.md`.

**PR breakdown:**

1. **Device frame and three-row grid.** `Insert::grid`, the type in the title, the new rows and
   cursor movement, and the device widget. It keeps today's graphics, unchanged in size, in the new
   panel. *The biggest PR: roughly 600 to 900 lines with tests.*
2. **Bypass toggle.** The switch in the title, faint contents, the faint curve variant, and
   `Off`/`On` as switches. *Small: about 100 to 150 lines.*
3. **Bigger, smooth graphics.** `Picture` alpha, the anti-aliased line drawn at cell pixels, the
   13×5 comp curve with its grid and threshold, the 5-row GR meter with its scale and held mark, the
   33×5 EQ, compression and throttling, and screenshots. *Medium: about 400 lines.*
4. **Later.** The chain line and scrolling past four slots, the ball on the curve, the C-style zoom
   on a focused device, and graphics for Distort, Delay and Reverb.

## 8. Open questions for the user

1. **The type in the title bar.** Is it right to move Comp's `Type` (Glue), Distort's and Chorus's
   type, and LPG's and Comb's `Mode` into the title next to the effect's name, reached with `k`, and
   take them off the grid?
2. **Switch glyphs for `Off`/`On` settings.** Should two-state settings (Auto, Peak Clip, Ping-pong,
   Freeze) use the same toggle glyph as bypass, or stay as words?
3. **Option C's big view later.** Do you want it as a zoom key on top of A (the focused device full
   page, with a 36×14 curve and the GR history), or is A's 13×5 curve big enough?
