//! `vimtracker probe`: one voice or effect from a spec, with no song (decision 110).

use std::path::PathBuf;

use vt_tui::dsp_probe::{self, Input, Loaded, ProbeError, frames_in, parse, prepare, render};

const ACID_SPEC: &str = include_str!("../../../tools/compare/specs/acid-cutoff.toml");
const COMP_SPEC: &str = include_str!("../../../tools/compare/specs/comp-static.toml");

fn loaded(text: &str) -> Loaded {
    Loaded {
        spec: parse(text).expect("the spec parses"),
        folder: PathBuf::new(),
    }
}

fn rendered(text: &str, case: Option<&str>) -> Vec<f32> {
    render(prepare(&loaded(text), case).expect("the spec prepares"))
}

fn refused(text: &str) -> String {
    match parse(text) {
        Err(ProbeError::Spec(message)) => message,
        other => panic!("expected the spec refused, got {other:?}"),
    }
}

fn peak(samples: &[f32]) -> f32 {
    samples
        .iter()
        .fold(0.0, |most, sample| most.max(sample.abs()))
}

/// A voice's spec with these settings under `[ours]`.
fn acid(settings: &str) -> String {
    format!(
        "duration = 0.25\n\
         [[notes]]\ntime = 0.05\nlength = 0.1\nnote = 36\n\
         [ours]\nmachine = \"acid\"\n{settings}\n"
    )
}

/// An effect's spec given a sine, with these lines under `[ours]`.
fn sine_through(ours: &str) -> String {
    format!(
        "[input]\nsignal = \"sine\"\nfrequency = 1000.0\nlevel = -6.0\nseconds = 0.1\n\
         [ours]\n{ours}\n"
    )
}

#[test]
fn the_example_specs_parse_and_every_case_prepares() {
    for text in [ACID_SPEC, COMP_SPEC] {
        let loaded = loaded(text);
        assert!(!loaded.spec.cases.is_empty());
        for case in &loaded.spec.cases {
            prepare(&loaded, Some(&case.name)).expect("the case prepares");
        }
    }
}

#[test]
fn a_probe_renders_the_same_every_time() {
    let first = rendered(ACID_SPEC, Some("cutoff-80"));
    assert_eq!(first.len(), 48_000 * 2, "the spec's duration, in stereo");
    assert!(peak(&first) > 0.01, "the note sounds");
    assert_eq!(first, rendered(ACID_SPEC, Some("cutoff-80")));

    let short_steps = COMP_SPEC.replace("hold = 0.25", "hold = 0.02");
    let compressed = rendered(&short_steps, Some("ratio-4"));
    assert_eq!(compressed, rendered(&short_steps, Some("ratio-4")));
}

#[test]
fn cases_change_the_sound() {
    let dark = rendered(ACID_SPEC, Some("cutoff-00"));
    let bright = rendered(ACID_SPEC, Some("cutoff-FF"));
    assert_ne!(dark, bright);
}

#[test]
fn an_input_with_no_effects_comes_out_as_it_went_in() {
    let loaded = loaded(&sine_through(""));
    let input = dsp_probe::signal(loaded.spec.input.as_ref().unwrap(), 48_000, &loaded.folder)
        .expect("a sine");
    let out = render(prepare(&loaded, None).unwrap());
    assert_eq!(out.len(), input.len() * 2, "as long as its input");
    for (frame, expected) in input.iter().enumerate() {
        assert!(
            (out[frame * 2] - expected[0]).abs() < 1e-6,
            "frame {frame} is unchanged, with no fade in"
        );
    }
}

#[test]
fn an_effect_changes_its_input() {
    let dry = rendered(&sine_through(""), None);
    let quieter = rendered(
        &sine_through("effects = [\"comp\"]\nsettings = { 1.comp.gain = 0x80 }"),
        None,
    );
    assert!(peak(&quieter) < peak(&dry) * 0.9);
}

#[test]
fn a_note_starts_at_its_time() {
    let out = rendered(&acid(""), None);
    let start = frames_in(0.05, 48_000);
    assert!(peak(&out[..start * 2]) == 0.0, "silent before the note");
    assert!(
        peak(&out[start * 2..(start + 2400) * 2]) > 0.01,
        "and sounding after"
    );
}

#[test]
fn stepped_levels_rise_a_step_at_a_time() {
    let text = "[input]\nsignal = \"steps\"\nfrequency = 1000.0\nfrom = -12.0\nto = 0.0\n\
                step = 6.0\nhold = 0.1\n[ours]\n";
    let loaded = loaded(text);
    let Some(input @ Input::Steps { .. }) = &loaded.spec.input else {
        panic!("stepped levels");
    };
    let frames = dsp_probe::signal(input, 48_000, &loaded.folder).unwrap();
    let held = frames_in(0.1, 48_000);
    assert_eq!(frames.len(), 3 * held, "-12, -6 and 0 dB");
    for (step, db) in [-12.0_f32, -6.0, 0.0].into_iter().enumerate() {
        let part = &frames[step * held..(step + 1) * held];
        let most = part
            .iter()
            .fold(0.0_f32, |most, frame| most.max(frame[0].abs()));
        assert!((most - 10_f32.powf(db / 20.0)).abs() < 1e-3, "{db} dB");
    }
}

#[test]
fn settings_take_the_programs_keys_bare_or_quoted_and_choices_by_name() {
    parse(&acid(
        "settings = { acid.cutoff = 0x40, \"acid.resonance\" = 255 }",
    ))
    .unwrap();
    parse(&acid("settings = { acid.wave = \"square\" }")).unwrap();
    parse(&acid("settings = { cut = 0x80, res = 0x20 }")).unwrap();
}

#[test]
fn an_unknown_setting_is_refused_with_the_ones_beside_it() {
    let message = refused(&acid("settings = { acid.cutof = 0x40 }"));
    assert!(
        message.contains("there's no setting called acid.cutof"),
        "{message}"
    );
    assert!(message.contains("acid.cutoff"), "{message}");

    let message = refused(&sine_through(
        "effects = [\"comp\"]\nsettings = { 1.comp.thresh = 0x40 }",
    ));
    assert!(message.contains("1.comp.threshold"), "{message}");
    assert!(
        !message.contains("1.delay"),
        "only the compressor's: {message}"
    );

    let message = refused(&acid("settings = { nonsense = 1 }"));
    assert_eq!(message, "there's no setting called nonsense");
}

#[test]
fn a_setting_the_spec_has_no_place_for_is_refused() {
    let message = refused(&acid("settings = { va.saw = 1 }"));
    assert!(
        message.contains("which the spec's machine isn't"),
        "{message}"
    );
    let message = refused(&sine_through("settings = { 1.comp.ratio = 0x40 }"));
    assert!(
        message.contains("which the spec's effects don't put there"),
        "{message}"
    );
    let message = refused(&acid("settings = { groove = 1 }"));
    assert!(message.contains("a probe has none of"), "{message}");
}

#[test]
fn values_out_of_range_and_unknown_choices_are_refused() {
    let message = refused(&acid("settings = { acid.cutoff = 256 }"));
    assert!(message.contains("out of its range"), "{message}");
    let message = refused(&acid("settings = { acid.wave = \"Sine\" }"));
    assert!(message.contains("Saw, Square"), "{message}");
    let message = refused(&acid("settings = { acid.cutoff = 0.5 }"));
    assert!(message.contains("is a float"), "{message}");
}

#[test]
fn a_spec_names_known_machines_effects_and_cases() {
    let message = refused(&acid("").replace("\"acid\"", "\"303\""));
    assert!(
        message.contains("there's no machine called 303"),
        "{message}"
    );
    let message = refused(&sine_through("effects = [\"compressor\"]"));
    assert!(message.contains("comp"), "{message}");
    let Err(ProbeError::Spec(message)) = prepare(&loaded(ACID_SPEC), None) else {
        panic!("a spec with cases needs one chosen");
    };
    assert!(message.contains("cutoff-00"), "{message}");
    let Err(ProbeError::Spec(message)) = prepare(&loaded(ACID_SPEC), Some("cutoff-01")) else {
        panic!("an unknown case is refused");
    };
    assert!(message.contains("no case called cutoff-01"), "{message}");
}

#[test]
fn a_spec_plays_notes_or_an_input_and_says_how_long() {
    refused("[ours]\nmachine = \"acid\"\n");
    let message = refused("[[notes]]\ntime = 0.0\nlength = 0.1\nnote = 36\n[ours]\n");
    assert!(message.contains("duration"), "{message}");
    assert!(matches!(
        parse("rate = 48000\nmistake = 1\n[ours]\n"),
        Err(ProbeError::Parse(_))
    ));
}
