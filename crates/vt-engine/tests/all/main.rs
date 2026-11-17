//! The engine's behaviour tests, a module for each feature, built as one binary so that they link
//! once rather than once a file (#719). A new file of tests needs its `mod` line here.

use std::path::Path;

mod support;

mod accumulators;
mod arps;
mod bank;
mod beads;
mod bypassed_amp;
mod choke;
mod clouds;
mod compressors;
mod conditions;
mod echo;
mod engine;
mod envelope;
mod exciter;
mod files;
mod filter_settings;
mod four_op;
mod fx;
mod gate_length;
mod gates;
mod generators;
mod glide;
mod glide_mode;
mod goldens;
mod grid_mutes;
mod groove;
mod groups;
mod key_track;
mod lane_moves;
mod lanes;
mod lfo;
mod live_lengths;
mod lockable_lists;
mod macro_generators;
mod macro_voices;
mod macros;
mod marks;
mod mod_slots;
mod modulator_bypass;
mod mono;
mod normalize;
mod note_clicks;
mod nudge;
mod pedals;
mod phase;
mod phrase_jump;
mod phrase_order;
mod play_order;
mod play_start;
mod quantise;
mod rate_modulation;
mod realign;
mod recorder;
mod render;
mod resting;
mod returns;
mod reverbs;
mod rotary;
mod section_loops;
mod sections;
mod slices;
mod slowdown;
mod spans;
mod speed;
mod steps;
mod stutter;
mod supersaw;
mod synth;
mod tails;
mod tempo;
mod timing;
mod track_reset;
mod tracking;
mod trim;
mod tutorial_songs;
mod variations;
mod velocity_delta;
mod voice_mode;

/// Every file here has its `mod` line, as one without isn't built, and none sits beside this
/// folder, where it would link a binary of its own.
#[test]
fn every_file_of_tests_is_in_this_binary() {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let main = include_str!("main.rs");
    for name in rust_files(&tests.join("all")) {
        assert!(
            name == "main" || main.contains(&format!("\nmod {name};\n")),
            "tests/all/{name}.rs isn't built without `mod {name};` in main.rs"
        );
    }
    let alone = rust_files(&tests);
    assert!(
        alone.is_empty(),
        "{alone:?} in tests/ each link a binary of their own: move them into tests/all/"
    );
}

/// The names of the Rust files in a folder, without `.rs`.
fn rust_files(folder: &Path) -> Vec<String> {
    std::fs::read_dir(folder)
        .unwrap()
        .filter_map(|entry| {
            let path = entry.unwrap().path();
            Some(path.file_name()?.to_str()?.strip_suffix(".rs")?.to_owned())
        })
        .collect()
}
