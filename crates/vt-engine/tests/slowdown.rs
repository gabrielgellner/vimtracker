//! The slowdown check, which times a fixed kernel in the callback against its time at full speed
//! for the leaf beside the CPU figure.

use std::time::Duration;

use vt_engine::slowdown::{SlowdownCheck, full_speed, kernel, time_kernel};

#[test]
fn the_kernel_always_gives_the_same_answer_for_a_seed() {
    assert_eq!(kernel(1), kernel(1));
    assert_eq!(kernel(0), kernel(0));
    assert_ne!(kernel(1), kernel(2));
    // Pinned, so a change to the kernel is noticed, and `KERNEL_VERSION` raised with it.
    assert_eq!(kernel(1), 7_283_080_626_347_225_757);
}

#[test]
fn the_kernel_takes_microseconds() {
    let fastest = full_speed();
    assert!(fastest > Duration::ZERO);
    // A test build is slower, and a busy machine slower again, but never this slow.
    assert!(fastest < Duration::from_millis(1), "{fastest:?}");
    assert!(time_kernel() >= Duration::from_nanos(1));
}

#[test]
fn the_check_looks_four_times_a_second_of_audio() {
    let mut check = SlowdownCheck::new(Duration::from_micros(2), 48_000);
    // The first callback looks, so there's a figure from the start.
    assert!(check.after(512).is_some());
    let looks = (0..375).filter(|_| check.after(128).is_some()).count();
    // 375 callbacks of 128 frames are a second.
    assert_eq!(looks, 4);
}

#[test]
fn a_look_is_its_time_over_full_speed_up_to_eight() {
    let mut check = SlowdownCheck::new(Duration::from_micros(2), 48_000);
    let us = Duration::from_micros;
    assert_near(check.found(us(2)).factor, 1.0);
    // One slow look alone doesn't count, as an interrupted one would be.
    assert_near(check.found(us(7)).factor, 1.0);
    assert_near(check.found(us(7)).factor, 3.5);
    assert_near(check.found(us(5)).factor, 2.5);
    assert_near(check.found(us(100)).factor, 2.5);
    assert_near(check.found(us(100)).factor, 8.0);
    // Through the callback, timing the kernel itself.
    let mut check = SlowdownCheck::new(Duration::from_nanos(1), 48_000);
    let look = check.after(512).unwrap();
    assert_eq!(look.factor, 8.0);
    assert!(look.time > Duration::ZERO);
}

fn assert_near(factor: f64, expected: f64) {
    assert!(
        (factor - expected).abs() < 1e-9,
        "{factor} against {expected}"
    );
}
