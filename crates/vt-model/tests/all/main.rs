//! The model's behaviour tests, a module for each area, built as one binary so that they link once
//! rather than once a file (#719). A new file of tests needs its `mod` line here.
//!
//! The binary's allocator is the engine's `rt_guard`, so `render.rs` can count what a generator's
//! render allocates. It counts only while a test arms it, so the rest run as they would without.

use std::path::Path;

// The model can't depend on the engine, so this builds the engine's file itself, which uses
// nothing else of the engine's. Its own tests run here too, and show the guard is installed.
#[path = "../../../vt-engine/src/rt_guard.rs"]
mod rt_guard;

#[global_allocator]
static RT_GUARD: rt_guard::Guarded = rt_guard::Guarded;

mod analysis;
mod bloom;
mod convert;
mod follow;
mod generators;
mod history;
mod kit;
mod listed;
mod macro_generators;
mod macros;
mod marks;
mod mod_fields;
mod project;
mod random_shapes;
mod render;
mod sample;
mod seeding;
mod slices;
mod stable_names;

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
