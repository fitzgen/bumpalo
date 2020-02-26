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

    let bump = Bump::try_new().unwrap();
    let mut rng = rand::thread_rng();
    let layout = Layout::from_size_align(2, 2)?;

    for _ in 0..NUM_TESTS {
        if rng.gen() {
            GLOBAL_ALLOCATOR.toggle_state();
        } else if GLOBAL_ALLOCATOR.is_returning_null() {
            toggling_assert!(bump.try_alloc_layout(layout).is_err());
        } else {
            toggling_assert!(bump.try_alloc_layout(layout).is_ok());
        }
    }

    Ok(())
}

#[test]
#[cfg(feature = "collections")]
fn test_try_reserve() {
    if GLOBAL_ALLOCATOR.is_returning_null() {
        GLOBAL_ALLOCATOR.toggle_state()
    }

    let bump = Bump::try_new();

    toggling_assert!(bump.is_ok());

    let bump = bump.unwrap();
    let mut vec = Vec::<u8>::new_in(&bump);

    toggling_assert!(vec.try_reserve(10).is_ok());
    toggling_assert!(vec.try_reserve_exact(10).is_ok());

    GLOBAL_ALLOCATOR.toggle_state();

    toggling_assert!(vec.try_reserve(10).is_err());
    toggling_assert!(vec.try_reserve_exact(10).is_err());
}
