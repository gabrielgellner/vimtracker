//! Counts allocations in a block that must not allocate, in test builds only.
//!
//! Copied from gmuse's `crates/gmuse-engine/src/rt_guard.rs`. The count is per thread here, so
//! a test that allocates on purpose can't fail another test running at the same time.
//!
//! It counts rather than panics, because panicking inside a global allocator is its own hazard:
//! the unwinder allocates.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    /// Whether this thread is currently forbidden from allocating.
    ///
    /// `const`-initialised on purpose: a lazily initialised thread-local allocates on first
    /// touch, which is an allocation inside the allocator.
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static VIOLATIONS: Cell<usize> = const { Cell::new(0) };
}

pub struct Guarded;

unsafe impl GlobalAlloc for Guarded {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        note();
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        note();
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        note();
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        note();
        unsafe { System.alloc_zeroed(layout) }
    }
}

/// Record a violation if this thread is armed.
///
/// `try_with` rather than `with`: a thread-local is unavailable during its own destruction, and
/// panicking there would turn a tidy shutdown into an abort.
fn note() {
    let _ = ARMED.try_with(|armed| {
        if armed.get() {
            let _ = VIOLATIONS.try_with(|count| count.set(count.get() + 1));
        }
    });
}

/// Run `f` with allocation forbidden on this thread, and say how many times it allocated anyway.
pub fn forbidden<R>(f: impl FnOnce() -> R) -> (R, usize) {
    let before = VIOLATIONS.with(Cell::get);
    ARMED.with(|armed| armed.set(true));
    let out = f();
    ARMED.with(|armed| armed.set(false));
    (out, VIOLATIONS.with(Cell::get) - before)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The guard sees an allocation, which is the only way to trust a zero from it.
    #[test]
    fn the_guard_notices_an_allocation() {
        let (v, count) = forbidden(|| vec![0u8; 64]);
        assert!(!v.is_empty());
        assert!(count > 0, "the guard did not see an obvious allocation");
    }

    #[test]
    fn arithmetic_is_not_an_allocation() {
        let (sum, count) = forbidden(|| (0..1000u64).sum::<u64>());
        assert_eq!(sum, 499_500);
        assert_eq!(count, 0);
    }
}
