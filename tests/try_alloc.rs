use bumpalo::Bump;
#[cfg(feature = "collections")]
use bumpalo::collections::Vec;
use rand::Rng;

use std::alloc::{GlobalAlloc, Layout, System};
use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};

struct Allocator(AtomicBool);

impl Allocator {
    fn is_returning_null(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    fn toggle_state(&self) {
        let current = self.0.load(Ordering::SeqCst);

        self.0.store(!current, Ordering::SeqCst)
    }

    fn scoped_success<F>(&self, callback: F)
    where
        F: FnOnce(),
    {
        let returning_null = self.is_returning_null();

        if returning_null {
            self.toggle_state()
        }

        callback();

        if returning_null {
            self.toggle_state()
        }
    }
}

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if self.is_returning_null() {
            core::ptr::null_mut()
        } else {
            SYSTEM_ALLOCATOR.alloc(layout)
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if !self.is_returning_null() {
            SYSTEM_ALLOCATOR.dealloc(ptr, layout);
        }
    }

    unsafe fn realloc(
        &self,
        ptr: *mut u8,
        layout: Layout,
        new_size: usize
    ) -> *mut u8 {
        if self.is_returning_null() {
            core::ptr::null_mut()
        } else {
            SYSTEM_ALLOCATOR.realloc(ptr, layout, new_size)
        }
    }
}

#[global_allocator]
static GLOBAL_ALLOCATOR: Allocator = Allocator(AtomicBool::new(false));
static SYSTEM_ALLOCATOR: System = System;

macro_rules! toggling_assert {
    ($cond:expr $(, $args:tt)*) => {
        let result = $cond;

        // assert! may allocate on failure, so we must re-enable allocations
        // prior to asserting
        GLOBAL_ALLOCATOR.scoped_success(|| assert!(result $(, $args)*))
    };
}

#[test]
fn test_try_alloc_layout() -> Result<(), Box<dyn Error>> {
    const NUM_TESTS: usize = 32;

    if !GLOBAL_ALLOCATOR.is_returning_null() {
        GLOBAL_ALLOCATOR.toggle_state();
    }

    toggling_assert!(Bump::try_new().is_err());

    GLOBAL_ALLOCATOR.toggle_state();

    let bump = Bump::try_new().unwrap();

    // Bump preallocates space in the initial chunk, so we need to
    // use up this block prior to the actual test
    let layout = Layout::from_size_align(bump.chunk_capacity(), 1)?;

    toggling_assert!(bump.try_alloc_layout(layout).is_ok());

    let mut rng = rand::thread_rng();

    for _ in 0..NUM_TESTS {
        let layout = Layout::from_size_align(bump.chunk_capacity() + 1, 1)?;

        if rng.gen() {
            GLOBAL_ALLOCATOR.toggle_state();
        } else if GLOBAL_ALLOCATOR.is_returning_null() {
            toggling_assert!(bump.try_alloc_layout(layout).is_err());
        } else {
            toggling_assert!(bump.try_alloc_layout(layout).is_ok());
        }
    }

    #[cfg(feature = "collections")]
    {
        if GLOBAL_ALLOCATOR.is_returning_null() {
            GLOBAL_ALLOCATOR.toggle_state();
        }

        let bump = Bump::try_new().unwrap();

        if !GLOBAL_ALLOCATOR.is_returning_null() {
            GLOBAL_ALLOCATOR.toggle_state();
        }

        let mut vec = Vec::<u8>::new_in(&bump);

        let chunk_cap = bump.chunk_capacity();

        // Will always succeed since this size gets pre-allocated in Bump::try_new()
        toggling_assert!(vec.try_reserve(chunk_cap).is_ok());
        toggling_assert!(vec.try_reserve_exact(chunk_cap).is_ok());
        // Fails to allocate futher since allocator returns null
        toggling_assert!(vec.try_reserve(chunk_cap + 1).is_err());
        toggling_assert!(vec.try_reserve_exact(chunk_cap + 1).is_err());

        GLOBAL_ALLOCATOR.toggle_state();

        let mut vec = Vec::<u8>::new_in(&bump);

        // Will always succeed since this size gets pre-allocated in Bump::try_new()
        toggling_assert!(vec.try_reserve(chunk_cap).is_ok());
        toggling_assert!(vec.try_reserve_exact(chunk_cap).is_ok());
        // Succeeds to allocate further
        toggling_assert!(vec.try_reserve(chunk_cap + 1).is_ok());
        toggling_assert!(vec.try_reserve_exact(chunk_cap + 1).is_ok());
    }

    // Reset the allocator for the test harness to use
    if GLOBAL_ALLOCATOR.is_returning_null() {
        GLOBAL_ALLOCATOR.toggle_state();
    }

    Ok(())
}
