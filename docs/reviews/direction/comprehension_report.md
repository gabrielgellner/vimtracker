# Stage 6: Comprehension test

Target: `docs/designs/vimtracker.md`, focused on "Direction: a track is one sound" (lines 518-619) and "Milestones after M3" (lines 1033-1044). Standard: Re-Implementation.

**Verdict: fail against the standard for the focus sections.** An engineer given M4-M8 would have to make design choices on the data model, playback rules and routing. Line 743 limits the readiness bar to M1-M3, so failing may be acceptable for a direction statement, but the document never says when these gaps get settled. Line 730's "own short design" applies to the After G1 list and is not stated for M4-M10. The Marks rules and the milestone sequencing are the most urgent.

## Blockers

**1. "Slot" means two different things.** Lines 61, 535, 562, 579, 582, 615, 1038, 1041. The glossary's slot is one FX command and value in a step. Line 562's is one effect on a track. Line 582 makes the number a per-track setting, while lines 562 and 615 argue for a fixed 3, and these only stop contradicting if the reader guesses they are separate. They need different names and glossary entries.

**2. When a step names a parameter, nothing says what it binds to or what happens if the track lacks it.** Lines 257, 543, 558, 562, 579, 617, 796, 933. Phrases stay shareable across tracks, but a command's meaning now depends on the playing track. Undefined: `DEC40` on a track with no reverb, or `CUT` on a bus with no filter (ignored, refused or flagged); two effects declaring one parameter, or one effect loaded twice, given three uppercase characters; whether a command follows its effect when reordered; what happens to step values when `K` swaps an effect; whether line 933's load error survives. Line 558's "self-contained" is true for who controls a track, not for what a phrase means.

**3. Lock and glide rules can't be implemented without guessing.** Lines 598-602, 1038.
*   "Next value in that slot": with `CUT40` on row 0 and `RES80` on row 1, does `RES` end the `CUT` lock? The same applies to a glide's target.
*   The same parameter in two slots: which lock wins, and what remains when one ends?
*   "Next note": does `OFF` count? A note whose `CHA` fails? With `DEL`, the row or the delayed start?
*   Layers (601) omit glide. What does a glide leave after landing, and is "stays put" a lock or a hold?
*   Does a note mid-glide interrupt it?
*   "Wrapping through the loop": a chain holds several phrases. Sections and `:reset` raise the same question.
*   Starting mid-pattern, during a lock or glide, or pausing in one.
*   Glide's interpolation scale, swing on landing, and what "lands exactly" tests against.
*   A page edit sets the held value (336), and a hold replaces the base (601). Which wins while a hold is active?

**4. When M4-M10 run relative to G1 contradicts the document's gate.** Lines 73-84, 607-608, 730, 743, 1035. Line 743: "Work stops at Issue 23 until the G1 outcome is recorded." Line 607 suggests after M6, line 608 suggests after G1, and line 730 has G1 choosing next work. A reader can't tell whether M4 can start now, whether G1 is optional, or what Stop would do to M7-M10. Line 607 doesn't say what remains of Issues 17 and 19, or whether Issues 20-22 must finish first.

## Major

**5. M7 depends on M8.** Lines 553-556, 1040-1041. A reader must invent how a send track hosts a reverb in M7 and what `DLY` and `REV` do meanwhile. M5 also builds an FX page with nothing to show until M7 or M8.

**6. Nothing specifies how bus, send and mix tracks behave as tracks.** Lines 53, 530-531, 549-558, 1040. Undefined: a note on a track with no machine, and `DEL`, `RET`, `KIL`, `OFF`, `CHA` there; what triggers envelopes or Retrig; lock lifetime; how each kind is created; whether they count against `MAX_TRACKS`; what a new project contains; deleting the mix track, given line 284 and RNG by index (378); mute and solo through routing; level and pan on buses and sends. Bus, Send, Mix, Sound, Kit, Mark, Lock, Hold and Glide have no glossary entries.

**7. Routing has no contract.** Lines 530, 553-556, 562, 1040. Can a track use sends and a bus? Can a bus feed a bus or a send, or a send feed a send? Do sends enter the mix directly? How does a track address a third send, given one `delay` and one `reverb` field? Where's the limiter relative to mix? "Routing can't feed back" doesn't say if it's refused at edit time, or if a sidechain counts. Processing order, sidechain latency, keys, commands and data fields are all unstated.

**8. Stems are untestable as written.** Lines 537, 911, 915, 1037, 1039. Is `--stems`' path a folder or prefix, and how are files named? "Sum to the full render without sends" could mean no sends or sends disabled. Does M6's "still sum" keep the exception? After M7, do bus, send and mix get stems, pre- or post-bus? How do mute, solo and the limiter affect them? The render rule's silent pass doesn't define a pass or a song's end for a whole song.

**9. The `Sound` boundary and the fate of instruments are not defined.** Lines 527, 534-536, 1037. Which `Track` fields belong to `Sound`? Where does `Instrument.volume` go (385)? Sound and kit files have no format, location, key or command. Which tracks does `:kit` fill, and what if a kit has more sounds than tracks? Track names have no length, uniqueness, command or default. Nothing retires `:sample`, `:inst`, `:label`, `Space i K`, `K` on Instrument, Insert (instrument) or `;` in the instrument view. The browser doesn't say what it browses or which track it assigns to. `song.ron` changes without a `FORMAT_VERSION` step.

**10. The parameter list's four attributes can't drive the listed behaviours.** Lines 543, 579, 1038. `track_page.rs` needs value kinds, nonlinear stepping, small and large sizes, wrapping, parsed text and signed values. An FX value is two hex digits, but attack reaches 8000 ms, and `Type` and `Rate` aren't numeric. Missing: mapping to `00`-`FF`, which parameters are lockable or modulatable, and command names, such as `Low cut` in three characters. Line 531 says modulation reaches "any of these", contradicting line 376 for the utility filter. M5's test "every parameter works with each key from the list alone" isn't checkable.

**11. Promised items have no milestone.** Lines 538, 562, 582, 585, 608, 735, 1037-1043. Mixer overview, per-track slot count, the shape panel learning glide, character delay and granular, per-track speed, grooves and rounding rules, waveforms and `:bpm fit`. The score view sits at M9 though M4 makes it much more valuable, with no reason given.

**12. The Filters milestone and Issue 50 don't exist in this document.** Line 1035. Issues stop at 23.

**13. Line 520's claim that the rest "describes what is built today" isn't reliable.** Lines 241-246, 473-482, 855, 955, 984-997. The model sketch lists `bpm`, instrument attack and release, and generator links, none of which match `song.rs`. Line 855 says "The sweep stays live". `track_page.rs` is missing from the file list. M4 needs a true description of what's being removed.

**14. Effect state in real time is unaddressed.** Lines 276, 539, 562, 935. Delay, reverb and granular need large buffers, the callback can't allocate, and `K` can swap effects during playback. 2.25 s of stereo delay for 3 slots on 64 tracks is about 166 MB. Line 1040's "cost of an idle track is measured" sets no target.

**15. Section behaviour in M6 is thin.** Lines 260-263, 294, 735, 1039. How sections advance, whether the last loops, default length, how `Section.reset` maps to reset or carry, whether locks and glides cross a boundary, what "from the top" means, transport scope, song view keys and layout. M9 and M10 have no tests, and "used it in a jam" can't be verified by anyone else.

## Nits

**16.** Line 603 says "Empty slots show nothing", but the example shows `---00`. It presumably means no mark.

**17. Mark editing details.** `~`'s order; `~` on an event, empty slot or note column; in Visual, does each slot cycle separately; the mark with nothing above; whether `y`, `p`, `d`, rotate and Shape carry marks; the on-file form, as line 587 says "one small value" and line 490 one readable string per step.

**18. Overloaded words.** `.` is lock and repeat. "Marks" also at 737. "Sound" is a track kind, a type, a page and a saved item.

**19. `dd` and `p` for reordering (562).** Line 422's `p` overwrites, so moving an effect overwrites its neighbour. Do settings travel with it?

**20. Pages (566).** `H` and `L` are top and bottom of view (818), so the override needs stating. The split of `ROWS` isn't given. "Their own page" could mean one per slot. The track page has no `Space` number at line 806.

**21. "Core effects, in order" (562).** Build order or signal order? "Saturation and bitcrush": one effect or two?

**22. Unlinked outside references (527, 534, 547, 562, 616).** Plaits, Syntakt, Tonverk, Heat's Flow page, Griesinger, Freeverb and Maschine. "Sound locks" and "sample locks" aren't defined. "Stepping" (543) doesn't say it means the `=`, `-`, `+`, `_` keys.

**23. Sweep smoothing is left half-answered.** Line 581 says glide answers it, while lines 133 and 399-402 present it as open with a different fallback.
