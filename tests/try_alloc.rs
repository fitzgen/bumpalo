use bumpalo::Bump;
use rand::Rng;

use std::alloc::{GlobalAlloc, Layout, System};
use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};

struct Allocator(AtomicBool);

impl Allocator {
    fn is_returning_null(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    fn toggle_state(&self) {
        let current = self.0.load(Ordering::Relaxed);

        self.0.store(!current, Ordering::Relaxed)
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

#[test]
fn test_alt_allocations() -> Result<(), Box<dyn Error>> {
    const NUM_TESTS: usize = 32;

    let bump = Bump::try_new().unwrap();
    let mut rng = rand::thread_rng();
    let layout = Layout::from_size_align(2, 2)?;

    for i in 0..NUM_TESTS {
        if rng.gen() {
            GLOBAL_ALLOCATOR.toggle_state();
        } else if GLOBAL_ALLOCATOR.is_returning_null() {
            assert!(bump.try_alloc_layout(layout).is_err());
        } else {
            assert!(bump.try_alloc_layout(layout).is_ok());
        }
    }

    Ok(())
}
