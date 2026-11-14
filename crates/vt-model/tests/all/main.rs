//! The model's behaviour tests, a module for each area, built as one binary so that they link once
//! rather than once a file (#719). A new file of tests needs its `mod` line here.
//!
//! `tests/render.rs` stays a binary of its own: it installs a `#[global_allocator]` to count what a
//! render allocates, and a binary has only one.

mod analysis;
mod bloom;
mod convert;
mod follow;
mod generators;
mod history;
mod kit;
mod macro_generators;
mod macros;
mod marks;
mod project;
mod random_shapes;
mod sample;
mod seeding;
mod slices;
