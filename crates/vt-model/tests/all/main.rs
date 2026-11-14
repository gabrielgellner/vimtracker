//! The model's behaviour tests, a module for each area, built as one binary so that they link once
//! rather than once a file (#719). A new file of tests needs its `mod` line here.
//!
//! `tests/render.rs` stays a binary of its own: it installs a `#[global_allocator]` to count what a
//! render allocates, and a binary has only one.

use std::path::Path;

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
mod sample;
mod seeding;
mod slices;
mod stable_names;

/// Every file here has its `mod` line, as one without isn't built, and only `render.rs` sits
/// beside this folder, where a file links a binary of its own.
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
    assert_eq!(
        alone,
        ["render"],
        "a file in tests/ links a binary of its own: move it into tests/all/"
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
