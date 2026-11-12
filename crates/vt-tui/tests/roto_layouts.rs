//! The Roto-Control's layouts: every kind of device has one, which places each of its settings
//! once, on pages of at most eight knobs and eight buttons, in a musical order, with gaps only
//! where a layout says, and its off/on settings on buttons (issue #419).

use std::collections::{BTreeMap, BTreeSet};

use vt_model::song::{Song, Track};
use vt_roto::layout::{Slot, row_ends_cleanly};
use vt_roto::live::{Feel, MAX_PAGES};
use vt_roto::sysex::CONTROLS;
use vt_tui::roto::devices::{
    FAMILY_COLOURS, Family, every_kind_of_device, knob, live_device, placement, stored_setups,
};
use vt_tui::roto::layouts::{ALL, DeviceLayout, Setting, app_only, key, layout_of, resolve};
use vt_tui::track_page::{Control, DeviceSpec, Page};

/// One of each kind of device, with its track, its song and its layout.
fn kinds() -> Vec<(Song, usize, Page, usize)> {
    every_kind_of_device()
}

fn spec_of(song: &Song, at: usize, page: Page, slot: usize) -> DeviceSpec {
    page.devices(&song.tracks[at])[slot].clone()
}

/// What a device is called in a failure.
fn what(spec: &DeviceSpec, track: &Track) -> String {
    format!(
        "{} ({}) on {:?}",
        spec.identity,
        layout_of(spec, track).kind,
        track.kind
    )
}

/// Whether a control should have a knob on this track: every setting it has, but a filter's
/// `Mode` when its type has no modes.
fn has_knob(control: Control, track: &Track) -> bool {
    control.exists_on(track) && !(control == Control::FilterMode && !track.filter.has_modes())
}

fn is_toggle(control: Control, track: &Track, song: &Song) -> bool {
    knob(control, track, song).is_some_and(|knob| knob.feel == Feel::Switch)
}

#[test]
fn every_kind_of_device_has_a_layout_and_every_layout_a_kind_of_device() {
    let mut used = BTreeSet::new();
    for (song, at, page, slot) in kinds() {
        let track = &song.tracks[at];
        let spec = spec_of(&song, at, page, slot);
        used.insert(layout_of(&spec, track).kind);
    }
    let all: BTreeSet<&str> = ALL.iter().map(|layout| layout.kind).collect();
    assert_eq!(all.len(), ALL.len(), "each layout's kind is its own");
    assert_eq!(used, all, "every layout is some kind of device's");
}

#[test]
fn every_setting_is_placed_exactly_once_by_a_key_its_device_has() {
    for (song, at, page, slot) in kinds() {
        let track = &song.tracks[at];
        let spec = spec_of(&song, at, page, slot);
        let layout = layout_of(&spec, track);
        // Everything but what the app alone chooses: the machine, the effect and its type.
        let controls: Vec<Control> = spec
            .rows()
            .flatten()
            .flatten()
            .filter(|control| !app_only(*control, track))
            .collect();
        let keys: BTreeSet<&str> = controls.iter().map(|c| key(*c, track)).collect();
        assert_eq!(
            keys.len(),
            controls.len(),
            "keys are unique in {}",
            what(&spec, track)
        );
        let mut placed: BTreeMap<String, usize> = BTreeMap::new();
        for setting in layout.layout.settings() {
            let control = resolve(setting, &spec, track)
                .unwrap_or_else(|| panic!("{} has no {}", what(&spec, track), setting.key));
            *placed.entry(format!("{control:?}")).or_default() += 1;
        }
        for control in &controls {
            assert_eq!(
                placed.get(&format!("{control:?}")),
                Some(&1),
                "{control:?} in {}",
                what(&spec, track)
            );
        }
        assert_eq!(placed.len(), controls.len(), "{}", what(&spec, track));
        // Shown live or stored, each is on one knob or button, which has a knob if it should.
        for stored in [false, true] {
            let placement = placement(&spec, track, stored);
            let shown: Vec<Control> = placement
                .knobs
                .iter()
                .chain(&placement.buttons)
                .flatten()
                .map(|(control, _)| *control)
                .collect();
            assert_eq!(
                shown.len(),
                controls.len(),
                "{} stored {stored}",
                what(&spec, track)
            );
            for control in &controls {
                assert_eq!(shown.iter().filter(|c| *c == control).count(), 1);
                assert_eq!(
                    knob(*control, track, &song).is_some(),
                    has_knob(*control, track),
                    "{control:?} in {}",
                    what(&spec, track)
                );
            }
        }
    }
}

/// A layout's rows: each page's knobs and its buttons.
fn rows(layout: &DeviceLayout) -> Vec<(&'static str, &[Slot<Setting>; CONTROLS])> {
    layout
        .layout
        .pages
        .iter()
        .flat_map(|page| [(page.name, &page.knobs), (page.name, &page.buttons)])
        .collect()
}

#[test]
fn no_layout_has_a_gap_it_does_not_mean() {
    for layout in ALL {
        for page in layout.layout.pages {
            assert!(!page.name.is_empty(), "{}'s pages are named", layout.kind);
            assert!(
                page.knobs
                    .iter()
                    .chain(&page.buttons)
                    .any(|slot| slot.setting().is_some()),
                "{}'s {} page has something on it",
                layout.kind,
                page.name
            );
        }
        for (name, row) in rows(layout) {
            assert!(
                row_ends_cleanly(row),
                "{}'s {name}: nothing after its end",
                layout.kind
            );
            // A gap is between settings: one at a row's end would be its end.
            let last = row.iter().rposition(|slot| !matches!(slot, Slot::Unused));
            if let Some(last) = last {
                assert!(
                    row[last].setting().is_some(),
                    "{}'s {name} ends with a gap",
                    layout.kind
                );
            }
        }
        // Shown live, no page of knobs is empty.
        let live = layout.layout.live_knobs();
        assert_eq!(live.len() % CONTROLS, 0);
        for page in live.chunks(CONTROLS) {
            assert!(page.iter().any(Option::is_some), "{}", layout.kind);
        }
    }
}

#[test]
fn no_page_has_more_than_eight_knobs_and_eight_buttons() {
    for layout in ALL {
        for page in layout.layout.pages {
            assert_eq!(page.knobs.len(), CONTROLS);
            assert_eq!(page.buttons.len(), CONTROLS);
        }
        let pages = layout.layout.pages.len();
        assert!(pages <= usize::from(MAX_PAGES), "{}", layout.kind);
        let live = layout.layout.live_knobs().len() / CONTROLS;
        assert!(live <= usize::from(MAX_PAGES), "{} live", layout.kind);
    }
    for (song, at, page, slot) in kinds() {
        let track = &song.tracks[at];
        let spec = spec_of(&song, at, page, slot);
        for stored in [false, true] {
            let device = live_device(page, slot, &spec, track, &song, stored);
            assert!(device.pages() <= MAX_PAGES);
            assert_eq!(device.controls.len() % CONTROLS, 0);
            assert_eq!(device.switches.len() % CONTROLS, 0);
        }
    }
}

/// The keys on a layout's first page of knobs, a gap or the end as `-`.
fn first_knobs(layout: &DeviceLayout) -> Vec<&'static str> {
    layout.layout.pages[0]
        .knobs
        .iter()
        .map(|slot| slot.setting().map_or("-", |setting| setting.key))
        .collect()
}

fn layout_named(kind: &str) -> &'static DeviceLayout {
    ALL.iter()
        .copied()
        .find(|layout| layout.kind == kind)
        .unwrap_or_else(|| panic!("{kind}"))
}

/// The key on a page's button, if it has one there.
fn button_key(layout: &DeviceLayout, page: usize, button: usize) -> Option<&'static str> {
    layout.layout.pages[page].buttons[button]
        .setting()
        .map(|setting| setting.key)
}

#[test]
fn an_envelope_is_attack_decay_sustain_release_first_and_an_lfo_what_it_plays() {
    let envelope = layout_named("adsr");
    assert_eq!(
        first_knobs(envelope)[..5],
        ["attack", "decay", "sustain", "release", "depth"]
    );
    let lfo = layout_named("lfo");
    assert_eq!(
        first_knobs(lfo)[..5],
        ["rate", "shape", "depth", "phase", "deform"]
    );
    // Where each goes is on a page of its own, one arrow on.
    for layout in [envelope, lfo] {
        assert_eq!(first_knobs_of(&layout.layout.pages[1])[0], "dest");
        assert!(!first_knobs(layout).contains(&"dest"));
    }
    // Each restart is under what it starts again.
    assert_eq!(button_key(envelope, 0, 0), Some("reset"), "under Attack");
    assert_eq!(button_key(lfo, 0, 3), Some("mode"), "under Phase");
    // And a modulator's polarity under its depth, which it turns the way of.
    assert_eq!(
        button_key(envelope, 0, 4),
        Some("polarity"),
        "under the envelope's Depth"
    );
    assert_eq!(
        button_key(lfo, 0, 2),
        Some("polarity"),
        "under the LFO's Depth"
    );
    assert_eq!(button_key(layout_named("arpeggio"), 0, 0), Some("retrig"));
    // A filter's cutoff then resonance; a compressor's threshold then ratio; a 303's order.
    assert_eq!(
        first_knobs(layout_named("filter"))[..2],
        ["cutoff", "resonance"]
    );
    assert_eq!(
        first_knobs(layout_named("fx/comp-glue"))[..2],
        ["threshold", "glueratio"]
    );
    assert_eq!(
        first_knobs(layout_named("machine/acid"))[..5],
        ["cutoff", "resonance", "envmod", "decay", "accent"]
    );
    // Every track's output has `Level` on knob 1 and `Trim` on knob 5.
    for kind in ["output", "output/group", "output/return", "output/master"] {
        let knobs = first_knobs(layout_named(kind));
        assert_eq!((knobs[0], knobs[4]), ("level", "trim"), "{kind}");
    }
}

#[test]
fn every_effect_ends_its_first_page_with_what_comes_out() {
    for layout in ALL.iter().filter(|layout| layout.kind.starts_with("fx/")) {
        if layout.layout.pages.is_empty() {
            continue;
        }
        // `Mix` is knob 7 and `Gain` knob 8 wherever an effect has them.
        let knobs = first_knobs(layout);
        let keys: Vec<&str> = layout
            .layout
            .settings()
            .map(|setting| setting.key)
            .collect();
        if keys.contains(&"mix") {
            assert_eq!(knobs[6], "mix", "{}: {knobs:?}", layout.kind);
        }
        if keys.contains(&"gain") {
            assert_eq!(knobs[7], "gain", "{}: {knobs:?}", layout.kind);
        }
    }
}

#[test]
fn no_layout_chooses_a_machine_an_effect_or_its_type_as_the_app_builds_and_the_roto_plays() {
    for (song, at, page, slot) in kinds() {
        let track = &song.tracks[at];
        let spec = spec_of(&song, at, page, slot);
        let layout = layout_of(&spec, track);
        for setting in layout.layout.settings() {
            let control = resolve(setting, &spec, track).unwrap();
            assert!(
                !app_only(control, track),
                "{control:?} on the Roto in {}",
                what(&spec, track)
            );
        }
    }
    // Musical lists stay.
    let acid = layout_named("machine/acid");
    assert!(acid.layout.settings().any(|setting| setting.key == "wave"));
    let filter = layout_named("filter");
    assert!(
        filter
            .layout
            .settings()
            .any(|setting| setting.key == "mode")
    );
}

#[test]
fn a_device_keeps_to_one_page_with_its_buttons_on_the_first_unless_it_has_a_reason() {
    for layout in ALL {
        let pages = layout.layout.pages;
        let Some(first) = pages.first() else {
            continue;
        };
        let full =
            |row: &[Slot<Setting>; CONTROLS]| row.iter().all(|slot| slot.setting().is_some());
        for (at, page) in pages.iter().enumerate().skip(1) {
            let knobs = page.knobs.iter().any(|slot| slot.setting().is_some());
            let buttons = page.buttons.iter().any(|slot| slot.setting().is_some());
            // A page more only for knobs the first has no room for, or a modulator's `Dest`.
            assert!(
                !knobs || full(&first.knobs) || page.name == "Dest",
                "{}'s page {} has knobs the first had room for",
                layout.kind,
                at + 1
            );
            assert!(
                !buttons || full(&first.buttons),
                "{}'s page {} has buttons the first had room for",
                layout.kind,
                at + 1
            );
        }
    }
}

fn first_knobs_of(page: &vt_roto::layout::Page<Setting>) -> Vec<&'static str> {
    page.knobs
        .iter()
        .map(|slot| slot.setting().map_or("-", |setting| setting.key))
        .collect()
}

#[test]
fn off_on_settings_are_on_buttons_and_only_they_and_their_devices_are_stored() {
    for (song, at, page, slot) in kinds() {
        let track = &song.tracks[at];
        let spec = spec_of(&song, at, page, slot);
        let layout = layout_of(&spec, track);
        assert_eq!(layout.stored, layout.has_buttons(), "{}", layout.kind);
        let on_buttons: Vec<Control> = layout
            .layout
            .pages
            .iter()
            .flat_map(|page| page.buttons.iter().filter_map(Slot::setting))
            .filter_map(|setting| resolve(setting, &spec, track))
            .collect();
        for control in spec.rows().flatten().flatten() {
            assert_eq!(
                is_toggle(control, track, &song),
                on_buttons.contains(&control),
                "{control:?} in {}",
                what(&spec, track)
            );
        }
    }
}

#[test]
fn a_live_device_has_its_buttons_on_two_step_knobs_above_them() {
    let song = Song::new(1);
    let track = &song.tracks[0];
    let envelope = &Page::Mod.devices(track)[0];
    let device = live_device(Page::Mod, 0, envelope, track, &song, false);
    assert!(!device.stored);
    assert!(device.switches.is_empty());
    // Button 1's reset, under `Attack`, on the nearest free knob of the page, the seventh, as
    // the sixth is its curve.
    let reset = device.controls[6].as_ref().unwrap();
    assert_eq!(reset.feel, Feel::Switch);
    assert_eq!(reset.name.as_str(), "Reset");
    // Stored, it's on button 1, and knob 7 is free.
    let stored = live_device(Page::Mod, 0, envelope, track, &song, true);
    assert!(stored.stored);
    assert!(stored.controls[6].is_none());
    assert_eq!(stored.switches[0].as_ref().unwrap().name.as_str(), "Reset");
    assert_eq!(stored.key, "vimtracker:adsr");
}

#[test]
fn the_colour_table_has_every_family_once_each_its_own_scheme() {
    let families: BTreeSet<String> = FAMILY_COLOURS
        .iter()
        .map(|row| format!("{:?}", row.family))
        .collect();
    let schemes: BTreeSet<u8> = FAMILY_COLOURS.iter().map(|row| row.scheme).collect();
    assert_eq!(families.len(), FAMILY_COLOURS.len());
    assert_eq!(schemes.len(), FAMILY_COLOURS.len());
    for row in FAMILY_COLOURS {
        assert!(usize::from(row.scheme) < vt_roto::colour::SCHEMES.len());
        assert_eq!(row.family.scheme(), row.scheme);
        assert!(!row.holds.is_empty() && !row.name.is_empty());
    }
    assert_eq!(Family::Level.scheme(), 19);
}

#[test]
fn every_stored_setup_fits_the_roto_and_keeps_its_version() {
    let setups = stored_setups();
    let kinds: BTreeSet<&str> = ALL
        .iter()
        .filter(|layout| layout.stored)
        .map(|layout| layout.kind)
        .collect();
    assert_eq!(setups.len(), kinds.len());
    let hashes: BTreeSet<_> = setups.iter().map(|setup| setup.hash()).collect();
    assert_eq!(hashes.len(), setups.len());
    for setup in &setups {
        assert!(
            setup.key.starts_with(vt_roto::store::NAMESPACE),
            "{}",
            setup.key
        );
        assert!(setup.name.as_str().starts_with(vt_roto::store::NAME_PREFIX));
        assert!(setup.name.as_str().len() <= 12, "{}", setup.name);
        assert!(!setup.switches.is_empty(), "{} has buttons", setup.key);
        assert!(setup.knobs.len() <= 64 && setup.switches.len() <= 64);
        for switch in &setup.switches {
            assert!(!switch.name.is_empty());
            assert_eq!(switch.light_off, vt_roto::store::LIGHT_OFF);
        }
    }
    let again = stored_setups();
    let versions: Vec<u64> = setups.iter().map(|setup| setup.version()).collect();
    let versions_again: Vec<u64> = again.iter().map(|setup| setup.version()).collect();
    assert_eq!(versions, versions_again, "the same every time");
    let names: Vec<&str> = setups.iter().map(|setup| setup.name.as_str()).collect();
    for wanted in ["vt Glue", "vt ADSR", "vt LFO", "vt Output", "vt Delay"] {
        assert!(names.contains(&wanted), "{names:?}");
    }
}

#[test]
fn a_slot_can_give_a_setting_another_colour_and_turn() {
    use vt_model::action::TrackControl;
    use vt_tui::roto::devices::placed;
    use vt_tui::roto::layouts::Turn;
    let song = Song::new(1);
    let track = &song.tracks[0];
    let cutoff = Control::Base(TrackControl::Cutoff);
    let plain = Setting {
        key: "cutoff",
        family: None,
        turn: None,
    };
    let shown = placed(cutoff, &plain, track, &song).unwrap();
    assert_eq!(
        shown,
        knob(cutoff, track, &song).unwrap(),
        "as its kind has it"
    );
    let changed = Setting {
        family: Some(Family::Time),
        turn: Some(Turn::CentreNotch),
        ..plain
    };
    let shown = placed(cutoff, &changed, track, &song).unwrap();
    assert_eq!(shown.colour, Family::Time.scheme());
    assert_eq!(shown.feel, Feel::CentreNotch);
    // A list told to turn smoothly does, and an off/on setting stays two steps.
    let speed = Setting {
        key: "speed",
        family: None,
        turn: Some(Turn::Smooth),
    };
    assert_eq!(
        placed(Control::Speed, &speed, track, &song).unwrap().feel,
        Feel::Smooth
    );
    let reset = Control::Mod(0, vt_tui::track_page::ModField::Reset);
    let smooth_reset = Setting {
        key: "reset",
        ..speed
    };
    assert_eq!(
        placed(reset, &smooth_reset, track, &song).unwrap().feel,
        Feel::Switch
    );
    // The effects' pitches are a pitch's colour, by their slots.
    let pitch = ALL.iter().find(|layout| layout.kind == "fx/pitch").unwrap();
    let first = pitch.layout.pages[0].knobs[0].setting().unwrap();
    assert_eq!((first.key, first.family), ("pitch", Some(Family::Time)));
}

#[test]
fn the_guides_legend_is_the_colour_table() {
    let guide = include_str!("../guides/meter.md").to_lowercase();
    for row in FAMILY_COLOURS {
        let line = format!("| {} | {} |", row.name, row.holds).to_lowercase();
        assert!(guide.contains(&line), "the guide has {line}");
    }
}

#[test]
fn every_button_is_under_a_knob_so_the_roto_draws_its_label() {
    for layout in ALL {
        for page in layout.layout.pages {
            for (at, button) in page.buttons.iter().enumerate() {
                if let Some(setting) = button.setting() {
                    assert!(
                        page.knobs[at].setting().is_some(),
                        "{}'s {} has a knob over {}",
                        layout.kind,
                        page.name,
                        setting.key
                    );
                }
            }
        }
    }
}

#[test]
fn an_lfo_rate_turns_smoothly_and_a_note_value_stays_one() {
    use vt_model::action::edit;
    use vt_model::lfo::{Rate, note_rates};
    use vt_roto::live::KNOB_TOP;
    use vt_tui::roto::edits::{turned, value_text};
    use vt_tui::track_page::ModField;
    // A stored knob keeps at most 16 steps, and a note value has 24, so it's smooth, its value
    // shown as it turns.
    let mut song = Song::new(1);
    song.tracks[0].mods[3].lfo.rate = note_rates()[3];
    let rate = Control::Mod(3, ModField::Rate);
    assert_eq!(
        knob(rate, &song.tracks[0], &song).unwrap().feel,
        Feel::Smooth
    );
    let song = std::sync::Arc::new(song);
    let next = edit(&song, &turned(rate, 0, &song, KNOB_TOP).unwrap()).unwrap();
    // The knob goes from slow to fast, so its top is the fastest note value.
    let fastest = note_rates()[0];
    assert_eq!(next.tracks[0].mods[3].lfo.rate, fastest);
    assert!(matches!(next.tracks[0].mods[3].lfo.rate, Rate::Note(_)));
    assert_eq!(
        value_text(rate, &next.tracks[0], &next),
        fastest.to_string()
    );
}

#[test]
fn everything_the_roto_is_sent_is_ascii_its_font_can_draw() {
    use vt_roto::Label;
    use vt_tui::roto::edits::value_text;
    let drawn = |label: &Label, what: &str| {
        assert!(
            label
                .as_str()
                .chars()
                .all(|c| c == ' ' || c.is_ascii_graphic())
                && !label.as_str().contains('?'),
            "{what}: {label}"
        );
    };
    for (song, at, page, slot) in kinds() {
        let track = &song.tracks[at];
        let spec = spec_of(&song, at, page, slot);
        for stored in [false, true] {
            let device = live_device(page, slot, &spec, track, &song, stored);
            drawn(&device.name, "a device's name");
            for control in device.controls.iter().chain(&device.switches).flatten() {
                drawn(&control.name, "a name");
                for step in control.feel.step_names() {
                    drawn(&step, control.name.as_str());
                }
            }
        }
        // The values shown on a knob's screen, as the app writes them.
        for control in spec.rows().flatten().flatten() {
            let text = value_text(control, track, &song);
            assert!(Label::shows_whole(&text), "{control:?}: {text}");
        }
    }
    for setup in stored_setups() {
        drawn(&setup.name, "a stored setup");
    }
    // An LFO's phase, which the app writes in degrees.
    let song = Song::new(1);
    let phase = Control::Mod(3, vt_tui::track_page::ModField::Phase);
    let text = value_text(phase, &song.tracks[0], &song);
    assert_eq!(Label::new(&text).as_str(), "0deg", "{text}");
}

#[test]
fn an_lfos_deform_is_a_knob_with_a_centre_notch_that_sets_it() {
    use vt_model::action::edit;
    use vt_roto::live::KNOB_TOP;
    use vt_tui::roto::edits::{turned, value_text};
    use vt_tui::track_page::ModField;
    let song = std::sync::Arc::new(Song::new(1));
    let track = &song.tracks[0];
    // `Mod 4`, read second.
    let spec = &Page::Mod.devices(track)[1];
    let device = live_device(Page::Mod, 1, spec, track, &song, true);
    let knob = device.controls[4].as_ref().unwrap();
    assert_eq!(knob.name.as_str(), "Deform");
    let deform = Control::Mod(3, ModField::Deform);
    let next = edit(&song, &turned(deform, 0, &song, KNOB_TOP).unwrap()).unwrap();
    assert_eq!(next.tracks[0].mods[3].lfo.deform, 0xFF);
    assert_eq!(value_text(deform, &next.tracks[0], &next), "+FF");
    let next = edit(&song, &turned(deform, 0, &song, 0).unwrap()).unwrap();
    assert_eq!(next.tracks[0].mods[3].lfo.deform, -0xFF);
}

#[test]
fn a_modulators_polarity_button_is_lit_for_both_ways_and_a_press_flips_it() {
    use vt_model::action::edit;
    use vt_model::modulation::Polarity;
    use vt_roto::live::{KNOB_TOP, position_of};
    use vt_tui::roto::edits::turned;
    use vt_tui::track_page::ModField;
    let song = std::sync::Arc::new(Song::new(1));
    // Read `Mod 1`, `Mod 4`, `Mod 2`: an LFO, then an envelope, each stored with its buttons.
    for (device, slot, button, lit) in [(1, 3, 2, true), (2, 1, 4, false)] {
        let track = &song.tracks[0];
        let spec = &Page::Mod.devices(track)[device];
        let stored = live_device(Page::Mod, device, spec, track, &song, true);
        let switch = stored.switches[button].as_ref().unwrap();
        assert_eq!(switch.name.as_str(), "Polarity");
        assert_eq!(switch.feel, Feel::Switch);
        let on = if lit { position_of(1, 2) } else { 0 };
        assert_eq!(switch.position, on, "Mod {}", slot + 1);
        // A press is the switch turned to its other end, which chooses the other polarity.
        let polarity = Control::Mod(slot, ModField::Polarity);
        let to = if lit { 0 } else { KNOB_TOP };
        let next = edit(&song, &turned(polarity, 0, &song, to).unwrap()).unwrap();
        let wanted = if lit { Polarity::Uni } else { Polarity::Bi };
        assert_eq!(next.tracks[0].mods[slot].polarity, Some(wanted));
        let spec = &Page::Mod.devices(&next.tracks[0])[device];
        let shown = live_device(Page::Mod, device, spec, &next.tracks[0], &next, true);
        let switch = shown.switches[button].as_ref().unwrap();
        assert_eq!(switch.position == 0, lit, "its light follows");
        // Pressed where it already is, nothing happens.
        assert!(turned(polarity, 0, &next, if lit { 0 } else { KNOB_TOP }).is_none());
    }
}

#[test]
fn an_envelopes_curve_is_a_knob_of_three_after_its_depth() {
    use vt_model::action::edit;
    use vt_model::envelope::Curve;
    use vt_roto::live::{KNOB_TOP, position_of};
    use vt_tui::roto::edits::{turned, value_text};
    use vt_tui::track_page::ModField;
    assert_eq!(first_knobs(layout_named("adsr"))[5], "mode");
    let song = std::sync::Arc::new(Song::new(1));
    let track = &song.tracks[0];
    // `Mod 2`, read third, after `Mod 1` and `Mod 4`.
    let spec = &Page::Mod.devices(track)[2];
    for stored in [false, true] {
        let device = live_device(Page::Mod, 2, spec, track, &song, stored);
        let knob = device.controls[5].as_ref().unwrap();
        assert_eq!(knob.name.as_str(), "Curve");
        let steps: Vec<String> = knob
            .feel
            .step_names()
            .iter()
            .map(|name| name.as_str().to_owned())
            .collect();
        assert_eq!(steps, ["Exp", "Lin", "Snap", "Analog"]);
        assert_eq!(knob.position, 0);
    }
    // Turned, it chooses the curve, which its screen names.
    let curve = Control::Mod(1, ModField::Mode);
    let next = edit(&song, &turned(curve, 0, &song, KNOB_TOP).unwrap()).unwrap();
    assert_eq!(next.tracks[0].mods[1].adsr.curve, Curve::Analog);
    assert_eq!(value_text(curve, &next.tracks[0], &next), "Analog");
    let next = edit(&song, &turned(curve, 0, &song, position_of(1, 4)).unwrap()).unwrap();
    assert_eq!(next.tracks[0].mods[1].adsr.curve, Curve::Lin);
}

#[test]
fn an_ahd_is_attack_hold_decay_then_depth_and_curve_stored_with_an_adsrs_buttons() {
    use vt_model::action::edit;
    use vt_model::envelope::{Curve, Hold};
    use vt_model::modulation::ModKind;
    use vt_roto::live::KNOB_TOP;
    use vt_tui::roto::edits::{turned, value_text};
    use vt_tui::track_page::ModField;
    let ahd = layout_named("ahd");
    assert!(ahd.stored);
    assert_eq!(
        first_knobs(ahd)[..6],
        ["attack", "hold", "decay", "-", "depth", "mode"]
    );
    assert_eq!(button_key(ahd, 0, 0), Some("reset"), "under Attack");
    assert_eq!(button_key(ahd, 0, 4), Some("polarity"), "under Depth");
    assert_eq!(first_knobs_of(&ahd.layout.pages[1])[0], "dest");
    // `Mod 2` made an AHD has its layout, named for it, with its own settings on the knobs.
    let mut song = Song::new(1);
    song.tracks[0].mods[1].kind = ModKind::Ahd;
    let song = std::sync::Arc::new(song);
    let track = &song.tracks[0];
    let spec = &Page::Mod.devices(track)[2];
    assert_eq!(layout_of(spec, track).kind, "ahd");
    let device = live_device(Page::Mod, 2, spec, track, &song, true);
    assert_eq!(device.name.as_str(), "M2 AHD");
    assert_eq!(device.key, "vimtracker:ahd");
    let names: Vec<&str> = device.controls[..6]
        .iter()
        .map(|knob| knob.as_ref().map_or("-", |knob| knob.name.as_str()))
        .collect();
    assert_eq!(names, ["Attack", "Hold", "Decay", "-", "Depth", "Curve"]);
    assert_eq!(device.switches[0].as_ref().unwrap().name.as_str(), "Reset");
    // Its hold turns from the note at the bottom through the times, shown as it turns.
    let hold = Control::Mod(1, ModField::Hold);
    let next = edit(&song, &turned(hold, 0, &song, 0).unwrap()).unwrap();
    assert_eq!(next.tracks[0].mods[1].ahd.hold, Hold::Note);
    assert_eq!(value_text(hold, &next.tracks[0], &next), "Note");
    let next = edit(&song, &turned(hold, 0, &song, KNOB_TOP).unwrap()).unwrap();
    assert_eq!(next.tracks[0].mods[1].ahd.hold, Hold::Ms(2000));
    assert_eq!(value_text(hold, &next.tracks[0], &next), "2000 ms");
    // Its attack and curve are its own, and its ADSR is left as it was.
    let attack = Control::Mod(1, ModField::Attack);
    let next = edit(&song, &turned(attack, 0, &song, KNOB_TOP).unwrap()).unwrap();
    assert_eq!(next.tracks[0].mods[1].ahd.attack_ms, 8000);
    assert_eq!(next.tracks[0].mods[1].adsr, song.tracks[0].mods[1].adsr);
    let curve = Control::Mod(1, ModField::Mode);
    let next = edit(&song, &turned(curve, 0, &song, KNOB_TOP).unwrap()).unwrap();
    assert_eq!(next.tracks[0].mods[1].ahd.curve, Curve::Analog);
    // Its reset's button switches its own.
    let reset = Control::Mod(1, ModField::Reset);
    let next = edit(&song, &turned(reset, 0, &song, 0).unwrap()).unwrap();
    assert!(!next.tracks[0].mods[1].ahd.reset);
    assert!(next.tracks[0].mods[1].adsr.reset);
}

#[test]
fn a_drum_is_peak_body_decay_and_ratchet_then_depth_and_curve_stored_for_its_polarity() {
    use vt_model::action::edit;
    use vt_model::modulation::ModKind;
    use vt_roto::live::{KNOB_TOP, position_of};
    use vt_tui::roto::edits::{turned, value_text};
    use vt_tui::track_page::ModField;
    let drum = layout_named("drum");
    assert!(drum.stored);
    assert_eq!(
        first_knobs(drum)[..6],
        ["peak", "body", "decay", "ratchet", "depth", "mode"]
    );
    assert_eq!(
        button_key(drum, 0, 0),
        None,
        "no reset, as it strikes from full"
    );
    assert_eq!(button_key(drum, 0, 4), Some("polarity"), "under Depth");
    assert_eq!(first_knobs_of(&drum.layout.pages[1])[0], "dest");
    // `Mod 2` made a drum has its layout, named for it, with its own settings on the knobs.
    let mut song = Song::new(1);
    song.tracks[0].mods[1].kind = ModKind::Drum;
    let song = std::sync::Arc::new(song);
    let track = &song.tracks[0];
    let spec = &Page::Mod.devices(track)[2];
    assert_eq!(layout_of(spec, track).kind, "drum");
    let device = live_device(Page::Mod, 2, spec, track, &song, true);
    assert_eq!(device.name.as_str(), "M2 Drum");
    assert_eq!(device.key, "vimtracker:drum");
    let names: Vec<&str> = device.controls[..6]
        .iter()
        .map(|knob| knob.as_ref().map_or("-", |knob| knob.name.as_str()))
        .collect();
    assert_eq!(
        names,
        ["Peak", "Body", "Decay", "Ratchet", "Depth", "Curve"]
    );
    let ratchet = device.controls[3].as_ref().unwrap();
    let steps: Vec<String> = ratchet
        .feel
        .step_names()
        .iter()
        .map(|name| name.as_str().to_owned())
        .collect();
    assert_eq!(steps, ["1", "2", "3", "4", "5", "6", "7", "8"]);
    // Turned, each sets the drum's own, and leaves its ADSR as it was.
    let control = Control::Mod(1, ModField::Ratchet);
    let next = edit(
        &song,
        &turned(control, 0, &song, position_of(2, 8)).unwrap(),
    )
    .unwrap();
    assert_eq!(next.tracks[0].mods[1].drum.ratchet, 3);
    assert_eq!(value_text(control, &next.tracks[0], &next), "3");
    let body = Control::Mod(1, ModField::Body);
    let next = edit(&song, &turned(body, 0, &song, KNOB_TOP).unwrap()).unwrap();
    assert_eq!(next.tracks[0].mods[1].drum.body, 100);
    let decay = Control::Mod(1, ModField::Decay);
    let next = edit(&song, &turned(decay, 0, &song, KNOB_TOP).unwrap()).unwrap();
    assert_eq!(next.tracks[0].mods[1].drum.decay_ms, 8000);
    assert_eq!(next.tracks[0].mods[1].adsr, song.tracks[0].mods[1].adsr);
}
