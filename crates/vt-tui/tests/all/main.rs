//! The interface's behaviour tests, a module for each part of the program, built as one binary so
//! that they link once rather than once a file (#719). A new file of tests needs its `mod` line
//! here, and helpers that more than one file uses go in `common/`.

use std::path::Path;

mod common;
mod description;

mod ableton;
mod analysis;
mod audio_page;
mod automation;
mod bits;
mod cells;
mod chain;
mod command_line;
mod divisions;
mod explorer;
mod fx_commands;
mod fx_page;
mod fx_zoom;
mod gen_page;
mod grammar;
mod grids;
mod groups;
mod guides;
mod heard_generators;
mod keys;
mod kit_dialog;
mod kits;
mod lane_moves;
mod launch;
mod macro_panel;
mod macros;
mod marbles_rhythms;
mod mixer;
mod mod_page;
mod notes;
mod onsets;
mod patches;
mod phrase;
mod phrase_view;
mod pickers;
mod presets;
mod probe;
mod project;
mod recording;
mod ring;
mod rotate;
mod roto;
mod roto_generators;
mod roto_layouts;
mod roto_mix;
mod roto_pins;
mod sample_page;
mod samples;
mod sampling;
mod scale;
mod song_view;
mod sound_page;
mod tags;
mod tempo;
mod timeline;
mod to_nothing;
mod tonnetz;
mod track_page;
mod tracks;
mod variation;
mod view_dump;
mod walks;
mod weights;
mod wheel;
mod world_rhythms;

/// Every file here has its `mod` line, as one without isn't built, and so has every file of
/// `common/` in its `mod.rs`; and none sits beside this folder, where it would link a binary of
/// its own.
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
    let common = include_str!("common/mod.rs");
    for name in rust_files(&tests.join("all/common")) {
        assert!(
            name == "mod" || common.contains(&format!("\nmod {name};\n")),
            "tests/all/common/{name}.rs isn't built without `mod {name};` in common/mod.rs"
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
