//! `vimtracker probe`: one voice or effect from a spec, with no song (decision 110).

use std::path::PathBuf;

use vt_model::song::FilterType;
use vt_tui::offline::dsp_probe::{
    self, CONTROL_FRAMES, Input, Loaded, ProbeError, drawn_response, frames_in, parse, prepare,
    render, render_with_controls,
};

const ACID_SPEC: &str = include_str!("../../../../tools/compare/specs/acid-cutoff.toml");
const COMP_SPEC: &str = include_str!("../../../../tools/compare/specs/comp-static.toml");
const LADDER_SPEC: &str = include_str!("../../../../tools/compare/specs/filter-ladder.toml");
const TIMING_SPEC: &str = include_str!("../../../../tools/compare/specs/comp-timing.toml");
/// Specs in tools/compare, which must all parse and prepare, with the nuance suite's, whose cases
/// play notes of their own and name two references. `nuance-chirp-riff` isn't one: it's a stub
/// naming a song, which tools/compare turns into a spec with `vimtracker notes` (tests/notes.rs).
const SPECS: [&str; 28] = [
    include_str!("../../../../tools/compare/specs/nuance-accents.toml"),
    include_str!("../../../../tools/compare/specs/nuance-chirp.toml"),
    include_str!("../../../../tools/compare/specs/nuance-extremes.toml"),
    include_str!("../../../../tools/compare/specs/nuance-gate.toml"),
    include_str!("../../../../tools/compare/specs/nuance-knob-accent-apart.toml"),
    include_str!("../../../../tools/compare/specs/nuance-knob-accent.toml"),
    include_str!("../../../../tools/compare/specs/nuance-knob-cutoff-peak.toml"),
    include_str!("../../../../tools/compare/specs/nuance-knob-cutoff.toml"),
    include_str!("../../../../tools/compare/specs/nuance-knob-decay.toml"),
    include_str!("../../../../tools/compare/specs/nuance-knob-envmod.toml"),
    include_str!("../../../../tools/compare/specs/nuance-knob-resonance.toml"),
    include_str!("../../../../tools/compare/specs/nuance-legato.toml"),
    include_str!("../../../../tools/compare/specs/nuance-slides.toml"),
    ACID_SPEC,
    include_str!("../../../../tools/compare/specs/acid-filter.toml"),
    COMP_SPEC,
    include_str!("../../../../tools/compare/specs/comp-static-force.toml"),
    TIMING_SPEC,
    include_str!("../../../../tools/compare/specs/comp-timing-force.toml"),
    include_str!("../../../../tools/compare/specs/filter-clean.toml"),
    LADDER_SPEC,
    include_str!("../../../../tools/compare/specs/filter-ladder-level.toml"),
    include_str!("../../../../tools/compare/specs/filter-ours.toml"),
    include_str!("../../../../tools/compare/specs/filter-ringing.toml"),
    include_str!("../../../../tools/compare/specs/filter-ringing-ours.toml"),
    include_str!("../../../../tools/compare/specs/filter-refs-svf.toml"),
    include_str!("../../../../tools/compare/specs/filter-refs-ladder.toml"),
    include_str!("../../../../tools/compare/specs/filter-refs-ms20.toml"),
];

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
    for text in SPECS {
        let loaded = loaded(text);
        assert!(!loaded.spec.cases.is_empty());
        for case in &loaded.spec.cases {
            prepare(&loaded, Some(&case.name)).expect("the case prepares");
        }
    }
}

#[test]
fn a_probe_renders_the_same_every_time() {
    let first = rendered(ACID_SPEC, Some("cutoff-88"));
    assert_eq!(first.len(), 48_000 * 2, "the spec's duration, in stereo");
    assert!(peak(&first) > 0.01, "the note sounds");
    assert_eq!(first, rendered(ACID_SPEC, Some("cutoff-88")));

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
    let louder = rendered(
        &sine_through(
            "effects = [\"comp\"]\nsettings = { 1.comp.threshold = 0xFF, 1.comp.makeup = 0x80 }",
        ),
        None,
    );
    assert!(peak(&louder) > peak(&dry) * 1.5);
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

/// A quiet sine sweep through the track's filter, with these settings.
fn swept(settings: &str) -> String {
    format!(
        "[input]\nsignal = \"sweep\"\nfrom = 20.0\nto = 20000.0\nlevel = -30.0\nseconds = 0.2\n\
         [ours]\nsettings = {{ {settings} }}\n"
    )
}

#[test]
fn a_spec_sets_the_tracks_filter_type_by_its_name() {
    let probe = prepare(&loaded(&swept("filter = \"ladder\", cut = 0x80")), None).unwrap();
    assert_eq!(probe.song.tracks[0].filter, FilterType::Ladder);
    let probe = prepare(&loaded(&swept("filter = \"MS20 HP\"")), None).unwrap();
    assert_eq!(probe.song.tracks[0].filter, FilterType::Ms20HighPass);

    let clean = rendered(&swept("filter = \"LP\", cut = 0x80, res = 0xE6"), None);
    let ladder = rendered(&swept("filter = \"Ladder\", cut = 0x80, res = 0xE6"), None);
    assert_ne!(clean, ladder, "the type changes the sound");
    assert_eq!(
        ladder,
        rendered(&swept("filter = \"Ladder\", cut = 0x80, res = 0xE6"), None),
        "and renders the same every time"
    );

    let message = refused(&swept("filter = \"Moog\""));
    assert!(
        message.contains("no filter type called Moog") && message.contains("Ladder, Diode"),
        "{message}"
    );
    let message = refused(&swept("filter = 3"));
    assert!(message.contains("takes a filter type's name"), "{message}");
}

#[test]
fn a_case_can_set_the_filter_type() {
    let text = format!(
        "{}[[cases]]\nname = \"diode\"\nours = {{ filter = \"Diode\" }}\n",
        swept("cut = 0x80")
    );
    let probe = prepare(&loaded(&text), Some("diode")).unwrap();
    assert_eq!(probe.song.tracks[0].filter, FilterType::Diode);
}

#[test]
#[allow(clippy::float_cmp, reason = "silence is exact zeros")]
fn an_impulse_is_one_sample_then_silence() {
    let input = Input::Impulse {
        level: -20.0,
        seconds: 0.1,
    };
    let frames = dsp_probe::signal(&input, 48_000, &PathBuf::new()).unwrap();
    assert_eq!(frames.len(), frames_in(0.1, 48_000));
    assert!((frames[0][0] - 0.1).abs() < 1e-6, "-20 dB");
    assert!(frames[1..].iter().all(|frame| *frame == [0.0; 2]));
}

#[test]
fn a_ladder_at_full_resonance_rings_from_an_impulse() {
    let text = "[input]\nsignal = \"impulse\"\nlevel = -20.0\nseconds = 0.5\n\
                [ours]\nsettings = { filter = \"Ladder\", cut = 0x90, res = 0xFF }\n";
    let out = rendered(text, None);
    assert!(
        peak(&out[out.len() / 2..]) > 0.01,
        "still ringing a quarter of a second on"
    );
}

#[test]
fn a_case_changes_the_input_or_replaces_it() {
    let timing = loaded(TIMING_SPEC);
    let long = prepare(&timing, Some("fast-long")).unwrap();
    let short = prepare(&timing, Some("fast-short")).unwrap();
    assert_eq!(
        long.frames - short.frames,
        frames_in(0.5, 48_000) - frames_in(0.05, 48_000),
        "the short burst's case is that much shorter"
    );

    let text = format!(
        "{}[[cases]]\nname = \"click\"\ninput = {{ signal = \"impulse\", level = 0.0, seconds = 0.1 }}\n",
        sine_through("")
    );
    let probe = prepare(&loaded(&text), Some("click")).unwrap();
    assert_eq!(
        probe.frames,
        frames_in(0.1, 48_000),
        "another signal replaces the input"
    );

    let text = format!(
        "{}[[cases]]\nname = \"wrong\"\ninput = {{ frequency = \"loud\" }}\n",
        sine_through("")
    );
    let message = refused(&text);
    assert!(message.contains("in the case wrong"), "{message}");
    let text = format!(
        "{}[[cases]]\nname = \"notes\"\ninput = {{ level = 0.0 }}\n",
        acid("")
    );
    let message = refused(&text);
    assert!(message.contains("plays notes"), "{message}");
}

#[test]
fn the_drawn_response_is_the_sound_pages() {
    let probe = prepare(&loaded(&swept("cut = 0x90")), None).unwrap();
    let drawn = drawn_response(&probe);
    let at = |hertz: f32| {
        drawn
            .iter()
            .min_by(|a, b| {
                (a.0 / hertz)
                    .ln()
                    .abs()
                    .total_cmp(&(b.0 / hertz).ln().abs())
            })
            .map(|point| point.1)
            .unwrap()
    };
    assert!((drawn[0].0 - 20.0).abs() < 1e-3);
    assert!((drawn[drawn.len() - 1].0 - 20_000.0).abs() < 1.0);
    assert!(at(100.0).abs() < 0.5, "flat well under its cutoff");
    // Two equal poles at Res 0, as OSR and Filter SEM have it (decision 137).
    assert!((at(988.0) + 6.0).abs() < 1.0, "6 dB down at it");
    assert!(at(10_000.0) < -30.0, "and falling well over it");
}

#[test]
fn the_ladder_spec_renders_the_same_every_time() {
    let short = LADDER_SPEC
        .replace("seconds = 2.0", "seconds = 0.1")
        .replace("duration = 2.5", "duration = 0.2");
    let first = rendered(&short, Some("ladder-90-res90"));
    assert!(peak(&first) > 0.001);
    assert_eq!(first, rendered(&short, Some("ladder-90-res90")));
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

/// A spec whose cases each play their own notes, with no notes of its own, and two references.
const CASE_NOTES: &str = "duration = 0.5\n\
     [ours]\nmachine = \"acid\"\n\
     [[references]]\nname = \"one\"\nplugin = \"a.vst3\"\n\
     [[references]]\nname = \"two\"\nplugin = \"b.vst3\"\n\
     [[cases]]\nname = \"early\"\nnotes = [{ time = 0.05, length = 0.1, note = 36 }]\n\
     references.one = { cutoff = 0.5 }\n\
     [[cases]]\nname = \"late\"\nnotes = [{ time = 0.25, length = 0.1, note = 36 }]\n";

#[test]
fn a_case_plays_its_own_notes_instead_of_the_specs() {
    // The frame a note starts on, whose first sample is the attack's zero.
    let start = |out: &[f32]| out.iter().position(|sample| *sample != 0.0).unwrap() / 2 - 1;
    assert_eq!(
        start(&rendered(CASE_NOTES, Some("early"))),
        frames_in(0.05, 48_000)
    );
    assert_eq!(
        start(&rendered(CASE_NOTES, Some("late"))),
        frames_in(0.25, 48_000)
    );

    // Over the spec's own notes too, which a case without notes keeps.
    let spec = format!(
        "{}[[cases]]\nname = \"late\"\nnotes = [{{ time = 0.15, length = 0.05, note = 48 }}]\n\
         [[cases]]\nname = \"own\"\n",
        acid("")
    );
    assert_eq!(
        start(&rendered(&spec, Some("own"))),
        frames_in(0.05, 48_000)
    );
    assert_eq!(
        start(&rendered(&spec, Some("late"))),
        frames_in(0.15, 48_000)
    );
}

#[test]
fn case_notes_are_checked_and_need_a_voice() {
    let without = format!("{CASE_NOTES}[[cases]]\nname = \"none\"\n");
    let message = refused(&without);
    assert!(
        message.contains("none") && message.contains("notes"),
        "{message}"
    );

    let empty = CASE_NOTES.replace(
        "notes = [{ time = 0.25, length = 0.1, note = 36 }]",
        "notes = []",
    );
    assert!(refused(&empty).contains("late"));

    let held_for_nothing =
        CASE_NOTES.replace("time = 0.25, length = 0.1", "time = 0.25, length = 0.0");
    assert!(refused(&held_for_nothing).contains("length"));

    let effect = format!(
        "{}[[cases]]\nname = \"notes\"\nnotes = [{{ time = 0.0, length = 0.1, note = 36 }}]\n",
        sine_through("")
    );
    assert!(refused(&effect).contains("input"));
}

/// A sine through the ladder at `Cut` 988 Hz, with these mod slots.
fn ladder_moved(mods: &str) -> String {
    sine_through(&format!(
        "[ours.settings]\nfilter = \"Ladder\"\ncut = 0x90\nres = 0x40\n{mods}"
    ))
}

fn controls(text: &str) -> (Vec<f32>, Vec<[f32; 2]>) {
    render_with_controls(
        prepare(&loaded(text), None).expect("the spec prepares"),
        true,
    )
}

#[test]
fn writing_down_the_controls_leaves_the_sound_as_it_is() {
    for text in [ladder_moved(""), acid("")] {
        let (heard, steps) = controls(&text);
        assert_eq!(heard, rendered(&text, None));
        assert_eq!(steps.len(), heard.len() / 2 / CONTROL_FRAMES);
    }
    let (_, steps) = controls(&ladder_moved(""));
    let (cut, res) = (144.0 / 255.0, 64.0 / 255.0);
    assert!(
        steps
            .iter()
            .all(|step| (step[0] - cut).abs() < 1e-6 && (step[1] - res).abs() < 1e-6),
        "a filter nothing moves is heard where it's set"
    );
}

#[test]
fn a_mod_slots_lfo_moves_the_filter() {
    let lfo = "[ours.mods.4]\nkind = \"Lfo\"\ndestination = \"cut\"\ndepth = 64\n\
               lfo = { shape = \"Tri\", rate = { Hertz = 2000 }, phase = 0, mode = \"Free\" }\n";
    let (moved, steps) = controls(&ladder_moved(lfo));
    assert_ne!(moved, rendered(&ladder_moved(""), None));
    let (low, high) = steps.iter().fold((1.0_f32, 0.0_f32), |(low, high), step| {
        (low.min(step[0]), high.max(step[0]))
    });
    let held = 144.0 / 255.0;
    // At 20 Hz, 0.1 s is two cycles, each a quarter of the knob either way.
    assert!(high > held + 0.2 && low < held - 0.2, "{low} to {high}");
}

#[test]
fn a_mod_slots_envelope_follows_the_notes() {
    let text = "duration = 0.5\n\
                [[notes]]\ntime = 0.0\nlength = 0.1\nnote = 36\n\
                [[notes]]\ntime = 0.25\nlength = 0.1\nnote = 36\n\
                [ours]\nmachine = \"va\"\n\
                [ours.settings]\nfilter = \"Ladder\"\ncut = 0x40\n\
                [ours.mods.2]\nkind = \"Adsr\"\ndestination = \"cut\"\ndepth = 128\n\
                adsr = { attack_ms = 1, decay_ms = 60, sustain = 0, release_ms = 20, reset = true }\n";
    let (_, steps) = controls(text);
    let at = |seconds: f64| steps[(seconds * 48_000.0) as usize / CONTROL_FRAMES][0];
    let rest = 64.0 / 255.0;
    for start in [0.0, 0.25] {
        assert!(
            at(start + 0.005) > rest + 0.3,
            "it opens as the note at {start} s starts"
        );
        assert!(
            (at(start + 0.2) - rest).abs() < 0.01,
            "and falls back by its end"
        );
    }
}

#[test]
fn mod_slots_are_numbered_from_one() {
    let slot = |number: &str| {
        format!(
            "{}[ours.mods.{number}]\nkind = \"Lfo\"\ndestination = \"cut\"\ndepth = 1\n\
             lfo = {{ shape = \"Tri\", rate = {{ Hertz = 100 }}, phase = 0, mode = \"Free\" }}\n",
            sine_through("")
        )
    };
    parse(&slot("1")).expect("the first slot");
    assert!(refused(&slot("0")).contains("numbered 1 to"));
    assert!(refused(&slot("9")).contains("numbered 1 to"));
}

#[test]
fn a_spec_drives_the_filter() {
    let driven = |drive: &str| {
        rendered(
            &sine_through(&format!(
                "[ours.settings]\nfilter = \"Ladder\"\ncut = 0xC0\ndrive = {drive}\n"
            )),
            None,
        )
    };
    assert_ne!(driven("0x00"), driven("0xFF"));
}
