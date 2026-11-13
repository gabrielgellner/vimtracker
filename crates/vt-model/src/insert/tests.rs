use super::*;
use crate::grid::GRID_COLUMNS;

#[test]
fn every_effect_starts_at_its_declared_defaults_and_fits() {
    for effect in Insert::ALL {
        assert!(effect.params().len() <= MAX_PARAMS, "{effect:?}");
        for (index, param) in effect.params().iter().enumerate() {
            assert_eq!(effect.value(index), param.default, "{}", param.name);
        }
    }
    assert_eq!(Insert::ALL[1], Insert::Distort(Distort::default()));
    for effect in crate::settings::Effect::ALL {
        assert!(effect.params().len() <= MAX_KEPT_PARAMS, "{effect:?}");
    }
}

/// Every effect in each of its types and modes, as it starts.
fn every_kind() -> Vec<Insert> {
    Insert::ALL
        .into_iter()
        .flat_map(|effect| match effect.type_param() {
            Some(at) => (0..effect.params()[at].choices.len())
                .map(|kind| effect.with_value(at, kind as u8))
                .collect(),
            None => vec![effect],
        })
        .collect()
}

#[test]
fn the_settings_that_are_off_or_on_are_toggles_and_no_others() {
    let mut toggles: Vec<&str> = every_kind()
        .iter()
        .flat_map(|effect| effect.params().iter())
        .filter(|param| param.is_toggle())
        .map(|param| param.name)
        .collect();
    toggles.sort_unstable();
    toggles.dedup();
    assert_eq!(
        toggles,
        [
            "Auto",
            "Fast",
            "Freeze",
            "Knock",
            "Mu-law",
            "Peak Clip",
            "Ping-pong",
            "Track",
            "Trig"
        ]
    );
    // A two-way list of anything else isn't one.
    let chorus = Insert::ALL
        .into_iter()
        .find(|effect| effect.name() == "Chorus")
        .unwrap();
    assert!(!chorus.params()[chorus.type_param().unwrap()].is_toggle());
}

#[test]
fn every_effects_grid_places_each_of_its_settings_once_with_what_comes_out_last() {
    for effect in every_kind()
        .into_iter()
        .filter(|effect| *effect != Insert::Empty)
    {
        let grid = effect.grid();
        let params = effect.params();
        assert!(
            grid.places_each_once(params.len(), effect.type_param()),
            "{effect:?}: {grid:?}"
        );
        // The column of what comes out is the last one used.
        let used = (0..GRID_COLUMNS)
            .filter(|column| (0..GRID_ROWS).any(|row| grid.at(row, *column).is_some()))
            .collect::<Vec<_>>();
        let last = *used.last().unwrap_or(&0);
        assert_eq!(
            used,
            (0..=last).collect::<Vec<_>>(),
            "{effect:?}: no empty column"
        );
        assert!(
            (0..GRID_ROWS)
                .filter_map(|row| grid.at(row, last))
                .any(|at| matches!(params[at].setting, Setting::Mix | Setting::Gain)),
            "{effect:?}: {grid:?}"
        );
        // A name has its column's ten characters and a space, and a choice the value's and
        // the bar's fourteen.
        for at in grid.settings() {
            assert!(params[at].name.len() < 10, "{}", params[at].name);
            for choice in params[at].choices {
                assert!(choice.chars().count() <= 14, "{choice}");
            }
        }
    }
    assert_eq!(Insert::Empty.grid(), Grid::default());
}

#[test]
fn a_few_effects_place_their_settings_by_what_they_mean() {
    let names = |effect: Insert, row: usize| {
        effect
            .grid()
            .row(row)
            .map(|at| at.map_or("", |at| effect.params()[at].name))
    };
    let glue = compressor(CompressorType::Glue);
    assert_eq!(names(glue, 0), ["Threshold", "Attack", "Makeup", "Mix"]);
    assert_eq!(names(glue, 1), ["Ratio", "Release", "Auto", "Peak Clip"]);
    assert_eq!(names(glue, 2), ["Range", "Source", "", ""]);
    assert_eq!(glue.type_param(), Some(0));
    let clean = compressor(CompressorType::Clean);
    assert_eq!(names(clean, 2), ["", "Source", "", ""]);
    let eq = Insert::Eq(Equaliser::DEFAULT);
    assert_eq!(names(eq, 0), ["Low freq", "Mid freq", "High freq", "Gain"]);
    assert_eq!(names(eq, 2), ["", "Mid Q", "", ""]);
    assert_eq!(eq.type_param(), None);
    let delay = Insert::Delay(Delay::DEFAULT);
    assert_eq!(names(delay, 2), ["Ping-pong", "Drive", "", "Gain"]);
    // The rest go in order, with what comes out after.
    let chorus = Insert::Chorus(Chorus::DEFAULT);
    assert_eq!(names(chorus, 0), ["Rate", "Mix", "", ""]);
    assert_eq!(names(chorus, 1), ["Depth", "Gain", "", ""]);
    let reverb = Insert::Reverb(Reverb::DEFAULT);
    assert_eq!(names(reverb, 0), ["Pre", "Low cut", "Mix", ""]);
    assert_eq!(names(reverb, 2), ["Damp", "", "", ""]);
}

/// A compressor of a type, as it starts.
fn compressor(kind: CompressorType) -> Insert {
    Insert::Compressor(Compressor {
        kind,
        ..Compressor::DEFAULT
    })
}

#[test]
fn a_compressor_starts_clean_and_each_type_shows_only_its_own() {
    assert_eq!(Compressor::default().kind, CompressorType::Clean);
    let names = |kind| {
        compressor(kind)
            .params()
            .iter()
            .map(|param| param.name)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(CompressorType::Clean),
        [
            "Type",
            "Source",
            "Threshold",
            "Ratio",
            "Attack",
            "Release",
            "Auto",
            "Mix",
            "Makeup"
        ]
    );
    assert_eq!(
        names(CompressorType::Console),
        [
            "Type",
            "Source",
            "Threshold",
            "Ratio",
            "Attack",
            "Release",
            "Auto",
            "Mix",
            "Makeup"
        ]
    );
    // The console's ratio, attack and release are its switches', with `Auto` after the times.
    let console = compressor(CompressorType::Console);
    let choices = |at: usize| console.params()[at].choices;
    assert_eq!(choices(3), ["2:1", "4:1", "10:1"]);
    assert_eq!(
        choices(4),
        ["0.1 ms", "0.3 ms", "1 ms", "3 ms", "10 ms", "30 ms"]
    );
    assert_eq!(choices(5), ["0.1 s", "0.3 s", "0.6 s", "1.2 s", "Auto"]);
    assert_eq!(console.value(3), 1, "4:1");
    assert_eq!(console.value(5), CONSOLE_AUTO_RELEASE);
    // Glue's are Live's, and it has its range, and its clip after everything else.
    assert_eq!(
        names(CompressorType::Glue),
        [
            "Type",
            "Source",
            "Threshold",
            "Ratio",
            "Attack",
            "Release",
            "Auto",
            "Mix",
            "Range",
            "Makeup",
            "Peak Clip"
        ]
    );
    let glue = compressor(CompressorType::Glue);
    let choices = |at: usize| glue.params()[at].choices;
    assert_eq!(choices(3), ["2:1", "4:1", "10:1"]);
    assert_eq!(
        choices(4),
        [
            "0.01 ms", "0.1 ms", "0.3 ms", "1 ms", "3 ms", "10 ms", "30 ms"
        ]
    );
    assert_eq!(
        choices(5),
        ["0.1 s", "0.2 s", "0.4 s", "0.6 s", "0.8 s", "1.2 s", "Auto"]
    );
    assert_eq!(GLUE_AUTO_RELEASE, 6);
    // It starts where Live loads it: 4:1, 1 ms, `.6`, all 70 dB of its range and no clip.
    assert_eq!(
        (
            glue.value(3),
            glue.value(4),
            glue.value(5),
            glue.value(8),
            glue.value(10)
        ),
        (1, 3, 3, 0xFF, 0)
    );
    let clip = glue.params()[10];
    assert_eq!(
        (clip.key, clip.label, clip.choices),
        ("peakclip", Some("PCLP"), &["Off", "On"][..])
    );
    assert_eq!(range_db(255.0), MOST_RANGE_DB);
    assert_eq!(range_db(0.0), 0.0);
}

#[test]
fn a_compressors_type_and_switches_save_and_load_and_an_old_one_loads_clean() {
    let console = Insert::Compressor(Compressor {
        kind: CompressorType::Console,
        console_ratio: 2,
        console_attack: 0,
        console_release: 3,
        ..Compressor::DEFAULT
    });
    let saved = ron::to_string(&console).unwrap();
    assert_eq!(ron::from_str::<Insert>(&saved).unwrap(), console);
    let glue = Insert::Compressor(Compressor {
        kind: CompressorType::Glue,
        glue_ratio: 2,
        glue_attack: 0,
        glue_release: GLUE_AUTO_RELEASE,
        range: 0x40,
        ..Compressor::DEFAULT
    });
    let saved = ron::to_string(&glue).unwrap();
    assert_eq!(ron::from_str::<Insert>(&saved).unwrap(), glue);
    // A compressor saved before it had types is the clean one, as it sounded.
    let old: Compressor = ron::from_str("(threshold: 170, ratio: 101, mix: 255)").unwrap();
    assert_eq!(old.kind, CompressorType::Clean);
    assert_eq!((old.threshold, old.ratio), (170, 101));
    // Each switch has a key of its own to save and lock it by.
    use crate::settings::{Effect, SettingId};
    let id = SettingId::from_key("1.comp.consolerelease").unwrap();
    assert_eq!(id.name(), "Release");
    assert_eq!(id.label().as_deref(), Some("1CREL"));
    let id = SettingId::from_key("1.comp.gluerelease").unwrap();
    assert_eq!(id.label().as_deref(), Some("1GREL"));
    let id = SettingId::from_key("1.comp.range").unwrap();
    assert_eq!((id.name(), id.label().as_deref()), ("Range", Some("1RNGE")));
    let at = Effect::Compressor
        .params()
        .iter()
        .position(|param| param.key == "type")
        .unwrap();
    assert_eq!(
        SettingId::slot(0, Effect::Compressor, at).unwrap().label(),
        None
    );
}

#[test]
fn changing_a_compressors_type_and_back_finds_each_types_settings() {
    let clean = compressor(CompressorType::Clean).write(Setting::Ratio, f32::from(0x30_u8));
    let bus = clean
        .write(Setting::CompressorType, 1.0)
        .write(Setting::ConsoleRatio, 9.0);
    assert_eq!(
        bus.read(Setting::ConsoleRatio),
        Some(2),
        "a switch stops at its last"
    );
    assert_eq!(
        bus.read(Setting::Ratio),
        Some(0x30),
        "kept while it's not shown"
    );
    let back = bus.write(Setting::CompressorType, 0.0);
    assert_eq!(back.read(Setting::Ratio), Some(0x30));
    assert_eq!(back.read(Setting::ConsoleRatio), Some(2));
}

#[test]
fn every_effect_ends_with_a_gain_and_a_compressor_with_its_makeup() {
    for effect in Insert::ALL.into_iter().filter(|e| *e != Insert::Empty) {
        let last = effect.params().last().unwrap();
        let expected = match effect {
            Insert::Compressor(_) => ("Makeup", 0),
            _ => ("Gain", UNITY_LEVEL),
        };
        assert_eq!((last.name, last.default), expected, "{effect:?}");
    }
    // Glue's clip comes after its makeup, as it does in what Glue does.
    let glue = compressor(CompressorType::Glue);
    let names: Vec<_> = glue.params().iter().rev().take(2).map(|p| p.name).collect();
    assert_eq!(names, ["Peak Clip", "Makeup"]);
}

#[test]
fn a_compressors_makeup_runs_from_unity_to_its_most_in_db() {
    assert_eq!(makeup_db(0.0), 0.0);
    assert_eq!(makeup_db(255.0), MOST_MAKEUP_DB);
    assert_eq!(MOST_MAKEUP_DB, 24.0);
    // Every type has it, with a label so steps lock it and LFOs move it over its whole range.
    for kind in CompressorType::ALL {
        let makeup = compressor(kind)
            .params()
            .iter()
            .find(|param| param.setting == Setting::Makeup)
            .copied()
            .unwrap();
        assert_eq!((makeup.key, makeup.label), ("makeup", Some("MKUP")));
        assert_eq!(Setting::Makeup.highest(), 0xFF);
        assert_eq!(Setting::Makeup.span(), 255.0);
    }
    let turned_up = compressor(CompressorType::Console).write(Setting::Makeup, 255.0);
    assert_eq!(turned_up.read(Setting::Makeup), Some(0xFF));
}

#[test]
fn a_delays_time_is_a_note_division_and_stops_at_the_ends() {
    let delay = Insert::ALL[2];
    let time = delay.params()[0];
    assert_eq!(time.choices[usize::from(delay.value(0))], "1/8d");
    assert_eq!(*time.choices.last().unwrap(), "1 bar");
    assert!(!time.wraps);
    assert_eq!(
        delay.write(Setting::Time, 300.0).read(Setting::Time),
        Some(17)
    );
}

#[test]
fn every_setting_of_every_effect_reads_as_it_writes() {
    for effect in Insert::ALL {
        for (at, param) in effect.params().iter().enumerate() {
            let highest = param.setting.highest();
            let written = effect.write(param.setting, f32::from(highest));
            assert_eq!(written.read(param.setting), Some(highest), "{}", param.name);
            // A type or mode shows its own settings, so the others are another list.
            if written.params() != effect.params() {
                continue;
            }
            for other in (0..effect.params().len()).filter(|other| *other != at) {
                let same = effect.params()[other].name;
                assert_eq!(written.value(other), effect.value(other), "{same}");
            }
        }
    }
}

#[test]
fn a_delay_saved_with_fewer_settings_loads_with_the_rest_as_new() {
    let delay: Delay = ron::from_str("(mix: 255, gain: 224)").unwrap();
    assert_eq!(
        delay,
        Delay {
            mix: 0xFF,
            ..Delay::DEFAULT
        }
    );
}

#[test]
fn a_saved_delay_keeps_its_settings() {
    let delay = Insert::RETURN_DELAY.with_value(1, 0x12);
    let text = ron::to_string(&delay).unwrap();
    assert_eq!(ron::from_str::<Insert>(&text).unwrap(), delay);
}

#[test]
fn a_reverbs_settings_change_alone_and_read_back() {
    let effect = Insert::ALL[3];
    let names: Vec<&str> = effect.params().iter().map(|param| param.name).collect();
    assert_eq!(
        names,
        ["Pre", "Decay", "Damp", "Low cut", "High cut", "Mix", "Gain"]
    );
    let mixed = effect.with_value(5, 0x12);
    assert_eq!((mixed.value(5), mixed.value(6)), (0x12, UNITY_LEVEL));
    assert_eq!(
        effect.write(Setting::Mix, 300.0).read(Setting::Mix),
        Some(0xFF)
    );
    assert_eq!(effect.read(Setting::Amount), None);
    assert_eq!(effect.stepped(true).stepped(false), effect);
}

#[test]
fn a_reverb_saved_with_fewer_settings_loads_with_the_rest_as_new() {
    let reverb: Reverb = ron::from_str("(mix: 255, gain: 224)").unwrap();
    assert_eq!(
        reverb,
        Reverb {
            mix: 0xFF,
            ..Reverb::DEFAULT
        }
    );
}

#[test]
fn a_distort_saved_without_a_gain_loads_at_unity() {
    let distort: Distort = ron::from_str("(kind: Fold, amount: 16, mix: 32)").unwrap();
    assert_eq!(distort.gain, UNITY_LEVEL);
}

#[test]
fn a_setting_changes_alone_and_reads_back() {
    let effect = Insert::ALL[1];
    for (index, param) in effect.params().iter().enumerate() {
        let value = if param.choices.is_empty() { 0x12 } else { 2 };
        let changed = effect.with_value(index, value);
        assert_eq!(changed.value(index), value, "{}", param.name);
        for other in (0..effect.params().len()).filter(|other| *other != index) {
            assert_eq!(changed.value(other), effect.value(other));
        }
    }
    assert_eq!(Insert::Empty.with_value(1, 0x12), Insert::Empty);
}

fn distort(kind: DistortType) -> Insert {
    Insert::Distort(Distort {
        kind,
        ..Distort::DEFAULT
    })
}

#[test]
fn no_two_settings_of_an_effect_share_a_name() {
    let modes = DistortType::ALL.map(|kind| distort(kind).params());
    for params in modes.into_iter().chain([&DISTORT_PARAMS[..]]) {
        for (at, param) in params.iter().enumerate() {
            for other in &params[at + 1..] {
                assert_ne!(param.name, other.name);
            }
        }
    }
}

#[test]
fn writing_lands_on_whole_values_inside_the_range() {
    let effect = distort(DistortType::Crush);
    assert_eq!(
        effect.write(Setting::Bits, 6.4).read(Setting::Bits),
        Some(6)
    );
    assert_eq!(
        effect.write(Setting::Bits, 6.6).read(Setting::Bits),
        Some(7)
    );
    assert_eq!(
        effect.write(Setting::Bits, -3.0).read(Setting::Bits),
        Some(0)
    );
    assert_eq!(
        effect.write(Setting::Bits, 99.0).read(Setting::Bits),
        Some(MOST_BITS - LEAST_BITS)
    );
    assert_eq!(
        effect.write(Setting::Type, 2.2).read(Setting::Type),
        Some(2)
    );
    assert_eq!(
        effect.write(Setting::Gain, 300.0).read(Setting::Gain),
        Some(MAX_LEVEL)
    );
}

#[test]
fn each_mode_has_its_own_setting_beside_mix_and_gain() {
    let names = |kind| -> Vec<&str> {
        distort(kind)
            .params()
            .iter()
            .map(|param| param.name)
            .collect()
    };
    assert_eq!(names(DistortType::Soft), ["Type", "Amount", "Mix", "Gain"]);
    assert_eq!(names(DistortType::Fold), ["Type", "Amount", "Mix", "Gain"]);
    assert_eq!(
        names(DistortType::Crush),
        ["Type", "Bits", "Mu-law", "Mix", "Gain"]
    );
    assert_eq!(
        names(DistortType::Down),
        ["Type", "Rate", "Track", "Mix", "Gain"]
    );
    for pedal in [DistortType::Overdrive, DistortType::Distortion] {
        assert!(pedal.is_pedal());
        assert_eq!(names(pedal), ["Type", "Amount", "Tone", "Mix", "Gain"]);
    }
    // Bits read as their names, and every one goes there and back.
    let bits = distort(DistortType::Crush).params()[1];
    assert_eq!(bits.choices[usize::from(bits.default)], "9");
    for index in 0..bits.choices.len() as u8 {
        assert_eq!(
            distort(DistortType::Crush).with_value(1, index).value(1),
            index
        );
    }
    assert!(!bits.wraps);
    assert_eq!(TYPE_PARAM.label, None, "a mode isn't locked from a step");
}

#[test]
fn changing_mode_keeps_each_modes_own_setting() {
    let crushed = distort(DistortType::Crush).with_value(1, 3);
    let soft = crushed.write(Setting::Type, 0.0).with_value(1, 0xF0);
    let back = soft.write(Setting::Type, DistortType::Crush as u8 as f32);
    assert_eq!(back.value(1), 3, "Bits as it was");
    assert_eq!(back.read(Setting::Amount), Some(0xF0));
}

#[test]
fn a_types_name_matches_its_place_in_the_choices() {
    for (index, kind) in DistortType::ALL.into_iter().enumerate() {
        assert_eq!(DISTORT_PARAMS[0].choices[index], kind.name());
    }
}

#[test]
fn a_phaser_and_a_flanger_sweep_in_note_lengths() {
    let phaser = Insert::Phaser(Phaser::DEFAULT);
    let flanger = Insert::Flanger(Flanger::DEFAULT);
    let rate = |effect: Insert| effect.params()[0].choices[usize::from(effect.value(0))];
    assert_eq!(rate(phaser), "2 bars");
    assert_eq!(rate(flanger), "1 bar");
    assert_eq!(*SWEEP_NAMES.last().unwrap(), "16 bars");
    assert_eq!(
        phaser
            .write(Setting::SweepRate, 300.0)
            .read(Setting::SweepRate),
        Some(SWEEP_NAMES.len() as u8 - 1)
    );
    let stages = phaser.params()[4];
    assert_eq!(stages.choices[usize::from(phaser.value(4))], "8");
    assert_eq!(
        phaser.write(Setting::Stages, 9.0).read(Setting::Stages),
        Some(2)
    );
}

#[test]
fn a_flangers_settings_read_in_real_units() {
    assert!((flanger_delay_ms(0.0) - 0.5).abs() < 1e-6);
    assert!((flanger_delay_ms(255.0) - 10.0).abs() < 1e-4);
    assert_eq!(flanger_feedback(128.0), 0.0);
    assert_eq!(flanger_feedback(0.0), -1.0);
    assert_eq!(flanger_feedback(255.0), 1.0);
    // It reads nothing for a setting it hasn't got, as a delay's feedback.
    let flanger = Insert::Flanger(Flanger::DEFAULT);
    assert_eq!(flanger.read(Setting::Feedback), None);
    assert_eq!(flanger.read(Setting::FlangerFeedback), Some(0xC0));
}

#[test]
fn a_tapes_drive_reads_in_db_around_the_middle() {
    assert_eq!(tape_drive_db(128.0), 0.0);
    assert_eq!(tape_drive_db(0.0), -MOST_TAPE_DRIVE_DB);
    assert_eq!(tape_drive_db(255.0), MOST_TAPE_DRIVE_DB);
    let tape = Insert::Tape(Tape::DEFAULT);
    // It reads nothing for a setting it hasn't got, as a delay's drive.
    assert_eq!(tape.read(Setting::Drive), None);
    assert_eq!(tape.read(Setting::TapeDrive), Some(0x80));
    assert_eq!(tape.read(Setting::Mix), Some(0xFF));
}

#[test]
fn a_plates_settings_share_the_reverbs_names_and_read_back() {
    let plate = Insert::Plate(Plate::DEFAULT);
    let names: Vec<&str> = plate.params().iter().map(|param| param.name).collect();
    assert_eq!(
        names,
        [
            "Pre", "Decay", "Damp", "High cut", "Size", "Mod", "Mix", "Gain"
        ]
    );
    let reverb_labels: Vec<_> = REVERB_PARAMS.iter().map(|param| param.label).collect();
    for shared in ["Pre", "Decay", "Damp", "High cut", "Mix", "Gain"] {
        let label = PLATE_PARAMS.iter().find(|param| param.name == shared);
        assert!(reverb_labels.contains(&label.unwrap().label), "{shared}");
    }
    let bigger = plate.with_value(4, 0xFF);
    assert_eq!(bigger.read(Setting::Size), Some(0xFF));
    assert_eq!(bigger.read(Setting::LowCut), None);
    let text = ron::to_string(&bigger).unwrap();
    assert_eq!(ron::from_str::<Insert>(&text).unwrap(), bigger);
    assert_eq!(plate_size(0.0), 0.5);
    assert_eq!(plate_size(128.0), 1.0);
    assert_eq!(plate_size(255.0), 2.0);
}

#[test]
fn a_pitch_shifters_settings_read_back_and_save() {
    let pitch = Insert::Pitch(Pitch::DEFAULT);
    let names: Vec<&str> = pitch.params().iter().map(|param| param.name).collect();
    assert_eq!(names, ["Pitch", "Fine", "Size", "Feedback", "Mix", "Gain"]);
    assert_eq!(pitch.read(Setting::Shift), Some(MOST_SHIFT));
    assert_eq!(PITCH_PARAMS[0].choices[usize::from(MOST_SHIFT)], "+0 st");
    let up = pitch.with_value(0, 36);
    assert_eq!(up.read(Setting::Shift), Some(36));
    assert_eq!(shift_semitones(36.0), 12.0);
    // It stops at two octaves.
    assert_eq!(pitch.with_value(0, 0xFF).read(Setting::Shift), Some(48));
    assert_eq!(pitch.read(Setting::Decay), None);
    let text = ron::to_string(&up).unwrap();
    assert_eq!(ron::from_str::<Insert>(&text).unwrap(), up);
    assert_eq!(fine_cents(0.0), -50.0);
    assert_eq!(fine_cents(128.0), 0.0);
    assert_eq!(fine_cents(255.0), 50.0);
    assert_eq!(grain_ms(0.0), 10.0);
    assert!((grain_ms(255.0) - 100.0).abs() < 1e-3);
    // Clouds' own grain, 2047 frames at 32 kHz.
    let clouds = 2047.0 / 32.0;
    assert!((grain_ms(f32::from(Pitch::DEFAULT.size)) - clouds).abs() < 0.5);
}

#[test]
fn a_granular_effects_settings_read_back_and_save() {
    let grains = Insert::Grains(Grains::DEFAULT);
    let names: Vec<&str> = grains.params().iter().map(|param| param.name).collect();
    assert_eq!(
        names,
        [
            "Time", "Size", "Density", "Spread", "Freeze", "Random", "Pitch", "Shape", "Mix",
            "Gain"
        ]
    );
    assert_eq!(grains.read(Setting::Shift), Some(MOST_SHIFT));
    assert_eq!(grains.read(Setting::Freeze), Some(0));
    let frozen = grains.with_value(4, 1);
    assert_eq!(frozen.read(Setting::Freeze), Some(1));
    // It's a switch.
    assert_eq!(grains.with_value(4, 0xFF).read(Setting::Freeze), Some(1));
    assert_eq!(grains.with_value(6, 0xFF).read(Setting::Shift), Some(48));
    assert_eq!(grains.read(Setting::Decay), None);
    let text = ron::to_string(&frozen).unwrap();
    assert_eq!(ron::from_str::<Insert>(&text).unwrap(), frozen);
    assert_eq!(grain_time_seconds(0.0), 0.0);
    assert_eq!(grain_time_seconds(255.0), 4.0);
    // Beads' `Size`: the whole recording backwards, the shortest at a third, and the whole of
    // it forwards.
    assert_eq!(grain_length(0.0), (4.0, true));
    let (shortest, _) = grain_length(85.0);
    assert!(shortest <= 0.008, "{shortest}");
    assert!(grain_length(84.0).1);
    assert!(!grain_length(86.0).1);
    let (longest, reversed) = grain_length(255.0);
    assert!((longest - 4.0).abs() < 1e-4 && !reversed, "{longest}");
    let (default, _) = grain_length(f32::from(Grains::DEFAULT.size));
    assert!((default - 0.15).abs() < 0.02, "{default}");
    assert_eq!(grain_density(0.0), Triggering::Regular(MOST_GRAIN_HERTZ));
    assert_eq!(grain_density(128.0), Triggering::Off);
    assert_eq!(
        grain_density(255.0),
        Triggering::Random(MOST_GRAIN_HERTZ * 0.5)
    );
}

#[test]
fn warps_settings_read_back_and_save_and_its_algo_reads_as_warps_knob() {
    let warps = Insert::Warps(Warps::DEFAULT);
    let names: Vec<&str> = warps.params().iter().map(|param| param.name).collect();
    assert_eq!(
        names,
        [
            "Algo", "Timbre", "Source", "Level", "Pitch", "Shape", "Mix", "Gain"
        ]
    );
    assert_eq!(WARPS_PARAMS[2].choices[0], "Osc");
    assert_eq!(WARPS_PARAMS[2].choices[1..], SOURCE_NAMES[1..]);
    assert_eq!(
        WARPS_PARAMS[4].choices[usize::from(warps.value(4))],
        "+12 st"
    );
    assert_eq!(WARPS_PARAMS[5].choices[usize::from(warps.value(5))], "Sine");
    assert_eq!(warps.with_value(5, 0xFF).read(Setting::Wave), Some(4));
    let fed = warps.with_value(2, 2);
    assert_eq!(
        match fed {
            Insert::Warps(warps) => warps.source_track(8),
            _ => None,
        },
        Some(1)
    );
    assert_eq!(Warps::DEFAULT.source_track(8), None);
    assert_eq!(warps.read(Setting::Shape), None);
    let text = ron::to_string(&fed).unwrap();
    assert_eq!(ron::from_str::<Insert>(&text).unwrap(), fed);
    // Each cross-modulation is an eighth further, on a value of its own, then the modulator,
    // then the vocoder.
    for (at, name) in ["Xfade", "Fold", "Ring", "Digital", "XOR", "Compare"]
        .into_iter()
        .enumerate()
    {
        let value = at as f32 * 32.0;
        assert_eq!(algorithm_blend(value), (at, at + 1, 0.0), "{name}");
        assert_eq!(ALGORITHM_NAMES[at], name);
    }
    let (from, to, by) = algorithm_blend(f32::from(0xB3_u8));
    assert_eq!((from, to), (5, 6));
    assert!(by > 0.95, "{by}");
    assert_eq!(algorithm_blend(255.0), (7, 7, 0.0));
}

#[test]
fn a_source_names_the_returns_by_their_old_numbers_and_the_groups_after() {
    // With 8 sound tracks, the groups are at 8 to 11, the returns at 12 and 13.
    let heard = |source: u8| source_track(source, 8);
    assert_eq!(SOURCE_NAMES[65], "A");
    assert_eq!(SOURCE_NAMES[66], "B");
    assert_eq!(heard(65), Some(8 + MAX_GROUPS));
    assert_eq!(heard(66), Some(9 + MAX_GROUPS));
    assert_eq!(SOURCE_NAMES[67], "G1");
    assert_eq!(heard(67), Some(8));
    assert_eq!(SOURCE_NAMES[66 + MAX_GROUPS], format!("G{MAX_GROUPS}"));
    assert_eq!(heard(66 + MAX_GROUPS as u8), Some(7 + MAX_GROUPS));
    assert_eq!(Setting::Source.highest(), 66 + MAX_GROUPS as u8);
}

#[test]
fn a_low_pass_gates_settings_read_back_and_save() {
    let gate = Insert::Lpg(LowPassGate::DEFAULT);
    let names: Vec<&str> = gate.params().iter().map(|param| param.name).collect();
    assert_eq!(
        names,
        [
            "Mode", "Amount", "Base", "Res", "Response", "Source", "Mix", "Gain"
        ]
    );
    assert_eq!(LPG_PARAMS[0].choices, ["Gate", "Filter", "VCA"]);
    assert_eq!(LPG_PARAMS[0].label, None, "a mode isn't locked from a step");
    // It goes round the modes, stopping at the last when written.
    assert_eq!(gate.with_value(0, 0xFF).read(Setting::GateMode), Some(2));
    let filter = gate.with_value(0, 1);
    assert!(matches!(
        filter,
        Insert::Lpg(LowPassGate {
            mode: GateMode::Filter,
            ..
        })
    ));
    let keyed = filter.with_value(5, 2);
    assert_eq!(
        match keyed {
            Insert::Lpg(gate) => gate.source_track(8),
            _ => None,
        },
        Some(1)
    );
    assert_eq!(LowPassGate::DEFAULT.source_track(8), None);
    assert_eq!(gate.read(Setting::Resonance), Some(0));
    assert_eq!(gate.read(Setting::Decay), None);
    let text = ron::to_string(&keyed).unwrap();
    assert_eq!(ron::from_str::<Insert>(&text).unwrap(), keyed);
}

#[test]
fn stepping_goes_round_the_effects_and_keeps_nothing() {
    let changed = Insert::ALL[1].with_value(1, 0x12);
    assert_eq!(changed.stepped(true), Insert::ALL[2]);
    let last = Insert::ALL[Insert::ALL.len() - 1];
    assert_eq!(last.stepped(true), Insert::Empty);
    assert_eq!(Insert::Empty.stepped(true), Insert::ALL[1]);
    assert_eq!(Insert::Empty.stepped(false), last);
}
