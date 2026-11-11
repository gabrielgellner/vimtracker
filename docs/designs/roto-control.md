# The Roto-Control: a second, separate way of working

**Status:** steps 1, 2 and 4 of milestone M30 (#410, #411 and #413), with decisions 158 to 160.
This note covers the whole milestone, #410 to #415, so the protocol layer fits what the later steps
need. Step 1 built the protocol, the link and `vimtracker roto-probe`; step 2 shows a track's
devices live on the Roto, storing nothing, behind an on/off setting on the audio page, off by
default (§4); step 4 makes its knobs edit the song, chooses its track and devices with its own
FUNC and SEL, and switches devices in and out (§4.2). §8 has the author's answers.

**The rule (the author's):** the Roto works independently of the screen. It's a separate,
supplementary way of working: the mixer can be on the Roto while the sample page is on screen, and a
knob can move an FX setting while the keyboard edits an instrument. So the Roto has its own view
of the song, which the cursor never moves unless the user asks it to (#415's follow-cursor option).

**Sources.** Melbourne Instruments' SysEx API v1.6 and serial API v1.3, its user manual, and its
MIT-licensed integrations for Ableton Live (`src/ableton/ROTO_CONTROL.py`), Bitwig Studio
(`src/bitwig`) and Logic Pro (`src/logic/config.lua`), in `../dsp/roto_control`. Where the PDFs and
the scripts disagree, the scripts win, as they run against the real firmware. The crate's `NOTICE`
carries the MIT notice.

**What the real Roto said** (the author's, firmware 3.3.0 `eaa37b0`, SysEx API 1.6, read on
2026-11-11 with `vimtracker roto-probe`, nothing written):

* MIDI ports `Roto-Control`, in and out. Two USB serial ports, `usbmodem1101` and `usbmodem1103`,
  both Melbourne Instruments `2e8a:f010`; only `1101` answers the serial API.
* The handshake took 456 ms to `ROTO-DAW CONNECTED`, then both versions came in 15 ms. The Roto
  pinged twice, so it connected twice; answering every ping is harmless.
* **It doesn't say which mode it's in on connecting.** It was in PLUGIN mode (serial `GET MODE`),
  but sent no `SET PLUGIN MODE`. The vendor's scripts push their state on connecting instead, and
  Bitwig's waits for a mode message before doing anything. So vimtracker sends its plugin list on
  connecting, which a Roto in PLUGIN mode shows at once (§4.1), and otherwise takes the mode from
  the messages MODE sends.
* **5 of its 64 PLUGIN setups are used**, by the author's Ableton devices (Drift, Delay, Chair of
  Boa, Acid V, Delay TAPE-2).
* A stored knob reads back in the 248 bytes the serial API's fields add up to, with two-byte values
  as 8-bit bytes, high first (index `00 16`, maximum `3F FF`): the widths the PDF leaves unsaid.

---

## 1. How the vendor's integrations talk to it

**The handshake** is the same in all three:

1. The program sends `DAW STARTED` (`0A 01`) when it starts, and nothing else until pinged.
2. The Roto sends `PING DAW` (`0A 02`), after `DAW STARTED`, and every second for a minute after
   it's plugged in.
3. The program answers `DAW PING RESPONSE` (`0A 03`) with which it is: Ableton `01`, Bitwig `02`,
   Logic `03`.
4. The Roto sends `ROTO-DAW CONNECTED` (`0A 0C`). Only now may the program send anything else.
5. Then each pushes its state. Ableton's starts its group-track timer, sends the selected track
   and the track list, and sets the meters' thresholds (87 and 113). Bitwig's asks for the firmware
   version and waits for a mode. vimtracker asks for the firmware and API versions, and reads the
   mode over serial.

**Which program vimtracker says it is: Bitwig Studio** (`02`). The ping's answer can only name
Ableton, Bitwig or Logic, and the Roto behaves a little differently for each. Logic's is furthest from what
vimtracker needs: its PLUGIN mode learns by sweeping a parameter, and its knobs' CC channels follow
the parameter's index. Ableton's ignores the program's page count for a macro plugin, as Live's
racks have a fixed 16 macros. Bitwig's is driven by the program throughout: a macro plugin has as
many pages as the program says, `MACRO PLUGIN PAGE INDEX` says which is shown, and `LEARN PARAM`
gives each control's name and feel as the page comes up, which suits devices whose settings are
defined in code. It also keeps vimtracker's session apart from Live's in what the Roto reports
(`GET CURRENT PLUGIN` says which program a plugin was for).

**Modes** come from the Roto when MODE is pressed: `SET MIX ALL TRACKS MODE` (`0C 01`, with which
tracks, what the knobs and switches do, and which send), `SET MIX TRACK MODE` (`0C 02`, one track),
`SET PLUGIN MODE` (`0B 01`). Pages and selections come the same way: `SET FIRST TRACK`, `ROTO
SELECT TRACK`, `SET FIRST PLUGIN`, `ROTO SELECT PLUGIN`, `MACRO PLUGIN PAGE INDEX`. The program
answers each with what that view shows: track details in eights, plugin details in eights.

**Knobs and switches** are CCs on channel 16, the same numbers in both modes: knob *n*'s high seven
bits on CC 12 + *n* and its low seven on 44 + *n*, high first; switch *n* on 20 + *n*; touch on 52 +
*n*; the eight transport buttons on 28 to 35, and the arrows on 36 and 37. The program moves a knob
by sending the same pair back, and lights a switch by sending its CC. MIX's meters are CCs to the
Roto, 65 + 2*n* and 66 + 2*n*, left and right, 0 to 127. CC 64 from the Roto, with a knob's number
as its value, asks for that knob's setting back at its default.

**What a knob shows.** In PLUGIN mode the Roto keeps a **setup** for each device, found by the
device's 8-byte hash: each knob's name, colour, and feel (smooth, stepped with up to 16 named steps,
or with a centre notch). When a device is chosen, the Roto sends `CONTROL MAPPED` for each control
on the page, and the program answers each with `LEARN PARAM`: its position, name, steps and step
names. Bitwig's script also has **macro plugins** (`PLUGIN DETAILS` type `01`, with a number of
pages): nothing is stored, and the program sends `LEARN PARAM` for each control of the page shown,
as the page comes up. `SHOW CONTROL VALUE` (`0A 18`) puts a value's text on a control's screen
until the Roto times it out.

**The errata** the crate follows are listed in `sysex.rs` and `serial.rs`: the PDF's decimal
headings for `1A` to `1D`, `ADD PLUGIN`'s code, `GET PLUGIN`'s and `GET KNOB CONTROL CONFIG`'s
lengths, `SET PLUGIN KNOB CONFIG`'s, the hex "10" step names, and the undocumented `0C 0C`, the
meters' lit states, which Ableton's script sends.

## 2. The Roto's own view

The Roto has a **view state** of its own in the app, beside the cursor and never the cursor:

```text
RotoView
  mode:      Mix { tracks: Playing | ReturnsAndMaster, first: track bank, knobs: Level | Pan | Send(n),
                   switches: Mute | Solo | Arm, focus: Option<track> }
           | Plugin { track, first: device bank, device, page }
  follow:    bool   (#415's follow-cursor option, off by default)
  shown:     what was last sent: names, colours, knob positions, lights
```

*   **What sets it** is the Roto alone: its MODE, arrows, SEL and touches arrive as the messages
    in §1 and change only `RotoView`. The keyboard, the cursor and the page on screen change it only
    with `follow` on, when the Roto shows the cursor's track and device.
*   **What it shows** is worked out from the song and `RotoView` each time either changes, and
    compared with `shown`, so only what differs is sent. A keyboard edit, an undo, a loaded song
    and a Roto edit all go the same way (#414).
*   **It isn't saved** in the song, as the cursor isn't. Whether it should be remembered between
    runs, in `settings.ron`, can wait for #415.

## 3. MIX and PLUGIN modes

**MIX mode is the mixer** (#412). "All tracks" shows the sound tracks then the groups, in banks of
8, each with its name and its group's colour (`colour::nearest` of the app's group colour), and the
groups marked as groups; "master-return" shows returns A and B and the master. Knob modes are Level,
Pan and Send A or B, the switch modes Mute and Solo. Arm has no meaning yet and stays dark. One
track's mode has Level and Pan on its first page and the sends after them, as Ableton's does; the
Roto names those knobs itself, so Trim, which it has no name for, is on the Output device in PLUGIN
mode instead. The meters are each track's peak from `Handle::latest`, the levels the mixer page
draws. The Roto shows up to 64 tracks in its manual; vimtracker has up to 64 sound tracks and 4
groups, so #412 checks how far `NUM TRACKS`' two bytes go before choosing what to leave off.

**PLUGIN mode is the device racks** (#413). The Roto's plugin list is one track's devices, in the
order the track page has them: the Sound page's Machine, Filter, Output and Play; the FX page's four
slots; the Mod page's envelopes, LFOs, Accumulator and Arpeggio. Each device's knobs are its
settings in the order of its `DeviceSpec`: the title's controls, then `Insert::grid()` or
`Machine::grid()` and the rest. A grid is 4 columns by 3 rows, so the knobs go row by row, holes
kept: knobs 1 to 4 are the top row and 5 to 8 the middle, and the second page is the bottom row and
the title's controls. A column's settings are then always a knob apart by four. A device's on/off,
an FX slot's bypass or a modulator's switch, is the plugin's enabled state, which SEL toggles.

**How each kind of setting feels**, from the settings registry's `Kind` (#411):

| Kind | Knob | Shown |
|---|---|---|
| `Stepped` with up to 16 choices | `KNOB_N_STEP`, a click for each, each named | the step's name |
| `Stepped` with 17 to 24 | stepped: `LEARN PARAM` carries names only for 16 steps or fewer | `SHOW CONTROL VALUE` with the choice, as for any stepped knob |
| `Stepped` with more, or that changes per track (`Dest`, the song's samples) | smooth, full range | `SHOW CONTROL VALUE` with the choice |
| An off/on toggle | a 2-step knob, its steps named `Off` and `On`; a macro plugin maps no switches | lit while on |
| `Continuous` with `middle` (Pan, Trim, Pitch, a depth, a band's boost) | centre notch | the value as the app formats it |
| Any other `Continuous` | `KNOB_300`, smooth | the value as the app formats it |

A position is the setting's value scaled to the knob's 0 to 16383: `value * 16383 / top`, and back,
rounded, so a knob turned one click of the app's lands where the key would have put it; a step is
its place among the steps, spread from 0 to 16383. An envelope's times go by the steps `=` takes (1
ms to 10 ms, 5 ms to 100 ms, then 25 ms), and an LFO's rate evenly in octaves from 64 bars a cycle
to 100 Hz, at 120 BPM for one in hertz, so every kind of rate has a place on one knob. Names are
the track page's labels, an effect's settings by their own names, cut to 12 characters by
`Label::new`, which none of today's needs. Colours are `colour::nearest` of the app's semantic
colours, by the setting's family wherever it is (decision 159): level and dynamics green, the
filter and the EQ blue, pan teal, time and pitch yellow, modulation mauve, character peach, and a
choice of machine, effect, sample or type in the text's colour.

## 4. The setup store

The Roto stores **64 PLUGIN setups**, shared by every program that drives it, each with up to 64
knobs and 64 switches (8 pages of 8), and **64 MIDI setups** of 32 knobs and switches. A stored knob
has up to 16 steps, all named; a knob sent in `LEARN PARAM` can have up to 24, named up to 16. Names
are 12 characters. The store keeps a setup by its hash, so vimtracker's are `PluginHash::of` a
stable key such as `vimtracker/fx/delay`, the same in every build.

**vimtracker's devices would take 55 of them**: 23 effect layouts (16 effects with one layout,
Distort's 4 and the compressor's 3), 22 machines (the sampler and 21 voices), and 10 track devices
(Filter, Output on a sound track, a group, a return and the master, Play, LFO, Envelope,
Accumulator, Arpeggio). With the author's 5 Ableton setups that's 60 of 64, and every device added
to vimtracker or learned in Ableton takes one more. **This is the reason for question 1**: the
Bitwig script's macro plugins store nothing, and send each control's name, steps, step names and
centre notch in `LEARN PARAM` as its page comes up, which covers everything in §3 but the colours.
`SET PLUGIN CONTROL DETAILS` (`0B 13`) sets a control's name and colour without storing it; the
PDF says it's "used normally in Logic Pro", and whether it works for a macro plugin is the first
thing #411 checks on the Roto.

**The author chose live devices** (question 1, §8), so nothing is stored: the serial API's SET,
CLEAR and factory reset commands are never sent, and the app opens no serial port at all.

### 4.1 Live devices, as built (#411)

**What vimtracker sends.** On `ROTO-DAW CONNECTED`, and again on each `SET PLUGIN MODE`, the
plugin list: `NUM PLUGINS`, `FIRST PLUGIN`, a `PLUGIN DETAILS` for each device of the bank, type
`01` (macro) with its number of pages, `PLUGIN DETAILS END`, then `DAW SELECT PLUGIN` with the
device and page chosen, not forced, so a device locked on the Roto stays. The Roto then sends
`CONTROL MAPPED` for each knob of the page shown, and each is answered with `LEARN PARAM` (its
name, centre notch, steps, step names and position, with the Roto's own number and hash given
back) and `SET PLUGIN CONTROL DETAILS` (its name and colour), or with `UNMAP CONTROL` for a hole.
`ROTO SELECT PLUGIN` (SEL) chooses another device, which is sent as the list again; the arrows
only move the page, as the Roto then asks for its knobs itself. As a stepped knob turns, its step's
name goes in `SHOW CONTROL VALUE`. This is
`vt-roto`'s `live::PluginView`, which has no I/O, and the app's `roto.rs` feeds it the link's
events once a frame.

**What the real Roto did** (the author's, firmware 3.3.0, 2026-11-11), each read back with the
serial API's `GET CURRENT PLUGIN` and `GET PLUGIN KNOB CONFIG`, which are read-only:

*   **It sends a byte the PDF doesn't have** with `SET PLUGIN MODE`, `0B 01 00`, which `sysex.rs`
    now reads (it was dropped as unreadable before). It says it after `DAW SELECT PLUGIN` changes
    what's shown, and not when the same is sent again, so answering it with the list again ends
    there.
*   **It asks for each knob, and only then listens.** After `DAW SELECT PLUGIN` it sends `CONTROL
    MAPPED` for the page's eight knobs, numbered from 1 across the device's pages (9 to 16 on the
    second), each with a hash of its own that's the same every time. `LEARN PARAM` sent before it
    asks, or for a knob of a page it isn't showing, was dropped. Answered, each knob read back with
    the name, the centre notch and the number of steps sent.
*   **Colours work in live mode.** `SET PLUGIN CONTROL DETAILS` takes the knob's number from 1,
    across the pages, as `LEARN PARAM` does, not its place on the page as the PDF's `00 - 3F`
    suggests: number 1 set the first knob, and 0 set nothing. It sets the name too, so it goes
    after `LEARN PARAM` with the same name. A knob not told a colour reads back as scheme 21.
*   **It shows no value of its own.** A stepped knob reads back with as many step names as steps,
    all empty, whatever `LEARN PARAM` sent, and the author saw no value on the screen as a knob
    turned. Ableton's script sends empty names too, saying they're "obtained live", and sends
    `SHOW CONTROL VALUE` (`0A 18`, the knob's place on the page) for every value as it changes. So
    as a stepped or off/on knob turns, vimtracker works out its step from the knob's position and
    sends its name (`PluginView::knob_turned`), which works past 16 steps too; the author saw
    each step's name flash up as the knob turned. A smooth knob's value is the setting's as the
    app formats it, which needs the knob to edit the song: #413 and #414.
*   **The author saw** the devices' names, each knob's name and colour, the steps and centre
    notches as sent, and the pages turning with the arrows.
*   **Its arrows say `PAGE LEFT` and `PAGE RIGHT`** (`0A 14`, `0A 15`) on a macro plugin, not
    `MACRO PLUGIN PAGE INDEX`, then ask for the new page's knobs. The page shown is taken from
    the knobs it asks for, 9 to 16 being the second, and the arrows too.
*   **No switches** were asked for: a macro plugin maps its knobs alone, so an off/on setting is a
    two-step knob. #413 tried to give it switches, and the Roto refused them (§4.2).
*   **Nothing was stored.** `GET CURRENT PLUGIN` read each device back as a `MacroAutoConfig` for
    Bitwig, `GET PLUGIN` by its hash found nothing, and the stored setups were the same five
    Ableton ones before and after every run.

### 4.2 Knobs that edit, and the Roto's own buttons, as built (#413)

**The buttons are the firmware's.** vimtracker follows the vendor's MODE, FUNC and SEL model
exactly as the manual has it for Live and Bitwig, and only fills in the lists and states the Roto
asks for. MODE switches between MIX, PLUGIN and MIDI. In PLUGIN mode:

*   **FUNC** shows the track list on the buttons (`NUM TRACKS`, `FIRST TRACK`, `TRACK DETAILS`,
    `TRACK DETAILS END`, then `DAW SELECT TRACK` for the one chosen, as Ableton's script sends
    them). The arrows page it (`SET FIRST TRACK`, answered with that bank), and a button chooses a
    track (`ROTO SELECT TRACK`), which PLUGIN mode then shows the devices of. This is the Roto's
    own choice of track, independent of the cursor: `vt-roto`'s `live::TrackList`. The list is
    every track in the song's order, a sound track as `Track 01` (or `01` and its name), a fixed
    one by its title; a group's tracks in its colour, and a group marked as one.
*   **Holding SEL** and pressing a button chooses a device (`ROTO SELECT PLUGIN`); its arrows page
    the devices in eights (`SET FIRST PLUGIN`).
*   **Tapping SEL** shows the devices' on/off states, and a button switches one
    (`SET PLUGIN ENABLED`). That's an FX slot's bypass or an envelope's or an LFO's switch, both
    ways: the Roto's switch is `b`'s action, and `b` in the app changes `PLUGIN DETAILS`' enabled
    flag, sent again. The machine, `Filter`, `Output`, `Play`, `Accumulator` and `Arpeggio` have
    no switch in vimtracker, so they're sent as always on, and an empty slot as off; a tap on one
    is ignored, and the list is sent again as it was.

**What the real Roto did** (the author's, firmware 3.3.0, 2026-11-11, from the app's log of
every message each way):

*   **Its buttons count from the bank shown.** On the second bank, `ROTO SELECT PLUGIN` and
    `SET PLUGIN ENABLED` for its second button carry `1`, not `9`, as both vendor scripts read
    them (`deviceBank.getItemAt(index)`, `_plugin_first_device + data[0]`). `PluginView::device_for`
    adds the bank's first device.
*   **The bank is the Roto's.** Sent again from the chosen device's bank, a toggle on the second
    bank turned the Roto back to the first. So the bank stays where its arrows left it, and
    `DAW SELECT PLUGIN` goes only when the chosen device is in the bank shown, as Bitwig's
    `sendPluginSelect` checks.
*   **A choice can come with a switch.** Tapping SEL, then holding it and pressing a device,
    switches the device as the button goes down, and as SEL is let go shows it and asks for its
    knobs, sometimes saying `ROTO SELECT PLUGIN` 3 to 4 seconds after the switch and sometimes
    not at all. Holding SEL from the Roto's normal view sends the choice alone, and a plain tap
    the switch alone, with no knobs asked for after it. Neither vendor script guards against it.
    In vimtracker a choice wins: after a `SET PLUGIN ENABLED`, within 10 seconds and with nothing
    else between, a `ROTO SELECT PLUGIN` for the same device, or the Roto asking for knobs that
    vimtracker didn't make it show, means it chose the device it switched. The switch is taken
    back, as if it was never made (`History::take_back`, no undo step and nothing to redo) unless
    the song has changed since, when it's switched back as an edit, and the device is answered
    with its own knobs. Without that, the Roto showed the device's name over the knobs of the one
    chosen before.
*   **SEL's views are the firmware's alone.** The Roto says nothing when SEL is held or let go, or
    which of its views is up: the API's `SET PLUGIN ENABLE MODE` and `SET PLUGIN SELECT MODE` are
    Logic Pro's only, and neither the Live nor the Bitwig script has them. So the buttons' lights
    can't be put out while SEL is held to choose. Holding SEL and letting it go also leaves the
    tapped view with nothing from vimtracker: the log has nothing sent at that moment. It's the
    vendor's behaviour, to raise with Melbourne Instruments if it's the same with Live; vimtracker
    hasn't been compared with Live.
*   **Switches on a live device are refused.** Answering a macro plugin with
    `SET PLUGIN CONTROL DETAILS` of type Switch, a `LEARN PARAM` numbered past its 64 knobs, and
    the buttons' light CCs gave the Roto no switch: it sent `CONTROL MAPPED` for knobs alone, and
    the serial API's read-only `GET PLUGIN SWITCH CONFIG` answered `NoPlugin` for each of the
    current device's switches. Only a stored setup has switches, and nothing is stored (§8,
    question 1), so Auto, Peak Clip, Ping-pong, Freeze, Reset and Retrig stay two-step knobs.
*   **A light flips as it's pressed.** Tapping `Accumulator` in the switches flips its light
    there and then, though the list sent straight back has it on; the next time the list comes up
    it's lit as sent. The firmware draws the press itself, and a value put on a button in the
    list (`SHOW CONTROL VALUE`, type Switch, tried with `Always on`) wasn't seen, so a device that
    can't be switched is left to its light.
*   **Knobs move with the CCs**: a knob's position sent as its CC pair turns its motor, and a
    value in `SHOW CONTROL VALUE` shows on its screen for a smooth knob as for a stepped one.
    `Output`'s `Level` typed from the keyboard as `00`, `E0`, `F8` and `E0` swung knob 5 to the
    bottom, near the top, the top and back, each at once ("the knobs are crazy responsive"), with
    `-inf dB`, `+0.0 dB` and `+6.0 dB` shown; stepped one at a time, it follows each step. A knob
    turned, let go and touched again didn't jump or push against the hand.

**Knobs edit the song** through `App::apply`, as keys do (`roto/edits.rs`, the way back from
`roto/devices.rs`). Each knob's last move of a frame is one action, on the Roto's track and device
whatever the cursor is on. A position becomes the value the mapping puts there: a list's step,
a value's share of its range with its fine digits, an envelope's time by `=`'s steps, and an
LFO's rate the nearest of its own kind, so a note division stays one. A knob left where its
setting is gives no action. Touch-on to touch-off is one undo step (`History::squash_to`, as
§5 has it), closed 300 ms after the last move with no knob touched, so a knob turned without a
touch is one step too. Two knobs held together make one step.

**What goes back.** After every new version of the song, from the Roto, the keys, an undo or a
loaded song, `PluginView::refresh` sends what changed: the plugin list if a device's name, state
or pages did, a knob learned again with the Roto's own hash if its name, feel or colour did, and
otherwise its position as CCs, with its value on its screen. A touched knob isn't moved, and
goes where its setting is once it's let go; a knob just turned takes its new position without
being sent it. Values, the app's own text (`edits::value_text`, the status line's units, or the
grid's text), are shown at most 12 times a second a knob, the last always.

## 5. Edits, feedback and rates

**The same path as keys.** A knob's move becomes the same `Action` its key would, through
`App::apply`, so it's one undo step and the engine hears it as any edit: `SetTrackLevel` for a
fader, `SetInsert` for an FX setting, `SetTrackSetting` for a machine's, and so on. Moves are
gathered each frame and only each knob's last is applied, so a fast turn is one edit a frame, not
one a CC. **A gesture is one undo step**: the Roto says when a knob is touched and let go, so the
edits between are squashed with `History::squash_to`, as a held key's are. A knob turned without a
touch, as a hard twist can be, closes its step after a short pause.

**Feedback.** After every new version, `RotoView` works out what the Roto should show and sends what
differs from `shown`: knob positions as CC pairs, lights as CCs, names and colours as SysEx.
**No echo**: a knob the user is touching isn't sent its own position, as its motor would fight the
hand, and the value it set is put in `shown` as it's applied, so it isn't sent back once let go.
`SHOW CONTROL VALUE` carries the value the app's grid shows, `-14.8 dB`, `Overdrive`, `1/16`, as a
knob turns or is touched.

**Rates**, from the vendor's scripts: values shown at most 12 times a second a control, as
Ableton's `VALUE_UPDATE_FPS`, with the last always sent; meters 12 times a second, each the highest
since the last, as its `AUDIO_METER_FPS`; at least 5 ms between SysEx messages, as Ableton's script
sleeps after each, which the link's thread keeps (`Options::sysex_gap`). Knob positions and lights
aren't paced but are sent once a frame at most, the newest only.

## 6. Unplugging, reconnecting and Ableton

**Unplugged.** The link's thread checks every second that the Roto's MIDI ports are there. When
they go, or a send fails, it says `Lost`, forgets the session and keeps watching; when they come
back it opens them and does the handshake again, and `RotoView` sends everything afresh on
`Connected`, as nothing on the Roto can be trusted to have survived. The view itself is kept, so it
comes back where it was. While it's away the app works as ever, and the audio page's Roto setting
says so (#415).

**Ableton.** On macOS the MIDI ports aren't exclusive, so Live and vimtracker can both have them
open, and both hear every knob. But the Roto has one session: a program's `DAW STARTED` makes it
ping, every program listening answers, and with two answering, both push their state and both act
on every knob. So **only one may drive it at a time**: vimtracker answers the Roto only while its
Roto setting is on, which is off by default (the author's answer to question 2, built in #411),
and Live's control surface should be set to None while it is. Turning vimtracker's off, or
quitting, sends `DAW STARTED` and then stays silent, so a Live that's running with the Roto's
script answers the pings and takes it back without a replug. The serial port is exclusive, but
only the Roto-Setup app and `roto-probe` use it; vimtracker itself never opens it.

## 7. Where the work happens

```text
 CoreMIDI's or ALSA's thread          the roto thread (vt-roto)            the edit thread (vt-tui)
 midir's input callback  --bytes-->   Splitter -> SysEx or CC              App::tick, once a frame:
                                      Session: handshake, versions  -->    Link::poll() -> Events
                                      ControlDecoder: 14-bit knobs  -->      -> RotoView -> Actions
                                      watches the ports every 1 s            -> App::apply
                                      sends, 5 ms between SysEx    <--     Link::send, send_ccs
                                                                           (what RotoView changed)
```

*   **The roto thread** owns the MIDI output and the session; no serial port, as nothing is
    stored (§4).
    Everything reaches it on one channel: the bytes midir's callback hands over, copied and nothing
    more, and what the edit thread asks to send. It answers pings itself, so the handshake doesn't
    wait for a frame.
*   **The edit thread** takes the thread's events in `App::tick`, as it takes the loader's and the
    analysis thread's, and never waits on it. A knob's move waits at most a frame, 33 ms, to reach
    the song.
*   **The audio thread** never hears of the Roto. Its levels for the meters are the telemetry the
    edit thread already has.
*   **Tests** use `fake::FakeRoto`, which speaks both protocols, and `fake::FakePorts`, which puts
    it where the system's MIDI ports would be, to plug in and unplug. No test needs hardware or a
    MIDI port; `roto-probe` is the check against the real one.

**Dependencies.** `midir` 0.11.1 for MIDI: the usual Rust crate for it, used by most Rust MIDI
programs, over CoreMIDI on macOS and ALSA on Linux, where it shares the `alsa` crate cpal already
links, so it adds no system library. `serialport` 4.10.1 for the serial port, the usual crate for
it, without its default `libudev`, so Linux needs nothing more either; it reads USB names from
IOKit on macOS and from `/sys` on Linux. Both are pinned, as every dependency is.

## 8. Questions for the author

**All four decided by the author on 2026-11-11** (decision 159):

1.  **Live devices, storing nothing on the Roto,** in Bitwig's mode, as recommended. Colours work
    there (§4.1).
2.  **Roto support is off by default.** Turning it on, on the audio page, takes the Roto; turning
    it off, or quitting, sends `DAW STARTED` and closes the link, so a running Live takes it back.
3.  **Knob order is row by row**, as recommended.
4.  **The extra transport buttons are unassigned for now**: session record, loop, punch in and
    out, and automation stay dark. Play, stop and record go to vimtracker's transport, and the
    arrows move between sections (#415).

The questions as they were put:

1.  **Stored setups or live devices?** #411 writes a stored PLUGIN setup for each device type,
    which takes 55 of the Roto's 64, leaving 4 beside your 5 Ableton ones and none for vimtracker's
    next devices. Claiming to be Bitwig, as step 1 does, lets each device be a macro plugin that
    stores nothing and sends its names, steps and centre notches as its page comes up; colours may
    have to wait on whether `SET PLUGIN CONTROL DETAILS` works there. **Recommended: live devices,
    with stored setups only if the Roto can't show them well.**
2.  **Sharing with Live.** One at a time, as §6 has it. Should vimtracker take the Roto whenever
    it's plugged in and the setting is on (so turning Live's surface to None is up to you), and is
    the setting **on or off by default**? Recommended: off by default, so opening vimtracker with
    Live running never takes the Roto away from it.
3.  **A device's knob order.** Row by row, as §3 has it (knobs 1 to 4 the grid's top row, 5 to 8
    its middle, the bottom row and the title on page 2), or down each column as the grid lists
    them? Recommended: row by row, so a column's settings are always four knobs apart.
4.  **The transport buttons.** PLAY, STOP and RECORD to vimtracker's play, stop and record, and the
    arrows to the previous and next section. Live's other four (session record, loop, punch in and
    out, automation) have no match: leave them dark, or give them vimtracker's own, such as fill
    held while pressed?
