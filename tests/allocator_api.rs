#![feature(allocator_api)]
#![cfg(feature = "allocator_api")]
#![cfg_attr(
    all(miri, feature = "test_skip_miri_quickchecks"),
    allow(unused_imports, dead_code)
)]
use bumpalo::Bump;
use quickcheck::quickcheck;

use std::alloc::{AllocError, Allocator, Layout};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

#[derive(Debug)]
struct AllocatorDebug {
    bump: Bump,
    grows: AtomicUsize,
    shrinks: AtomicUsize,
    allocs: AtomicUsize,
    deallocs: AtomicUsize,
}

impl AllocatorDebug {
    fn new(bump: Bump) -> AllocatorDebug {
        AllocatorDebug {
            bump,
            grows: AtomicUsize::new(0),
            shrinks: AtomicUsize::new(0),
            allocs: AtomicUsize::new(0),
            deallocs: AtomicUsize::new(0),
        }
    }
}

unsafe impl Allocator for AllocatorDebug {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.allocs.fetch_add(1, Relaxed);
        let ref bump = self.bump;
        bump.allocate(layout)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        self.deallocs.fetch_add(1, Relaxed);
        let ref bump = self.bump;
        bump.deallocate(ptr, layout)
    }

    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        self.shrinks.fetch_add(1, Relaxed);
        let ref bump = self.bump;
        bump.shrink(ptr, old_layout, new_layout)
    }

    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        self.grows.fetch_add(1, Relaxed);
        let ref bump = self.bump;
        bump.grow(ptr, old_layout, new_layout)
    }
}

#[test]
fn allocator_api_push_a_bunch_of_items() {
    let b = AllocatorDebug::new(Bump::new());
    let mut v = Vec::with_capacity_in(1024, &b);
    assert_eq!(b.allocs.load(Relaxed), 1);

    for x in 0..1024 {
        v.push(x);
    }

    // Ensure we trigger a grow
    assert_eq!(b.grows.load(Relaxed), 0);
    for x in 1024..2048 {
        v.push(x);
    }
    assert_ne!(b.grows.load(Relaxed), 0);

    // Ensure we trigger a shrink
    v.truncate(1024);
    v.shrink_to_fit();
    assert_eq!(b.shrinks.load(Relaxed), 1);

    // Ensure we trigger a deallocation
    assert_eq!(b.deallocs.load(Relaxed), 0);
    drop(v);
    assert_eq!(b.deallocs.load(Relaxed), 1);
}

#[test]
fn allocator_grow_zeroed() {
    // Create a new bump arena.
    let ref bump = Bump::new();

    // Make an initial allocation.
    let first_layout = Layout::from_size_align(4, 4).expect("create a layout");
    let mut p = bump
        .allocate_zeroed(first_layout)
        .expect("allocate a first chunk");
    let allocated = bump.allocated_bytes();
    unsafe { p.as_mut().fill(42) };
    let p = p.cast();

    // Grow the last allocation. This should just reserve a few more bytes
    // within the current chunk, not allocate a whole new memory block within a
    // new chunk.
    let second_layout = Layout::from_size_align(8, 4).expect("create a expanded layout");
    let p = unsafe { bump.grow_zeroed(p, first_layout, second_layout) }
        .expect("should grow_zeroed okay");
    assert!(bump.allocated_bytes() <= allocated * 2);
    assert_eq!(unsafe { p.as_ref() }, [42, 42, 42, 42, 0, 0, 0, 0]);
}

#[cfg(not(all(miri, feature = "test_skip_miri_quickchecks")))]
quickcheck! {
    fn allocator_grow_align_increase(layouts: Vec<(usize, usize)>) -> bool {
        let mut layouts: Vec<_> = layouts.into_iter().map(|(size, align)| {
            const MIN_SIZE: usize = 1;
            const MAX_SIZE: usize = 1024;
            const MIN_ALIGN: usize = 1;
            const MAX_ALIGN: usize = 64;

            let align = usize::min(usize::max(align, MIN_ALIGN), MAX_ALIGN);
            let align = usize::next_power_of_two(align);

            let size = usize::min(usize::max(size, MIN_SIZE), MAX_SIZE);
            let size = usize::max(size, align);

            Layout::from_size_align(size, align).unwrap()
        }).collect();

        layouts.sort_unstable_by_key(|l| (l.size(), l.align()));

        let b = AllocatorDebug::new(Bump::new());
        let mut layout_iter = layouts.into_iter();

        if let Some(initial_layout) = layout_iter.next() {
            let mut pointer = b.allocate(initial_layout).unwrap();
            if !is_pointer_aligned_to(pointer, initial_layout.align()) {
                return false;
            }

            let mut old_layout = initial_layout;

            for new_layout in layout_iter {
                pointer = unsafe { b.grow(pointer.cast(), old_layout, new_layout).unwrap() };
                if !is_pointer_aligned_to(pointer, new_layout.align()) {
                    return false;
                }

                old_layout = new_layout;
            }
        }

        true
    }

    fn allocator_shrink_align_change(layouts: Vec<(usize, usize)>) -> bool {
        let mut layouts: Vec<_> = layouts.into_iter().map(|(size, align)| {
            const MIN_SIZE: usize = 1;
            const MAX_SIZE: usize = 1024;
            const MIN_ALIGN: usize = 1;
            const MAX_ALIGN: usize = 64;

            let align = usize::min(usize::max(align, MIN_ALIGN), MAX_ALIGN);
            let align = usize::next_power_of_two(align);

            let size = usize::min(usize::max(size, MIN_SIZE), MAX_SIZE);
            let size = usize::max(size, align);

            Layout::from_size_align(size, align).unwrap()
        }).collect();

        layouts.sort_unstable_by_key(|l| l.size());
        layouts.reverse();

        let b = AllocatorDebug::new(Bump::new());
        let mut layout_iter = layouts.into_iter();

        if let Some(initial_layout) = layout_iter.next() {
            let mut pointer = b.allocate(initial_layout).unwrap();
            if !is_pointer_aligned_to(pointer, initial_layout.align()) {
                return false;
            }

            let mut old_layout = initial_layout;

            for new_layout in layout_iter {
                let res = unsafe { b.shrink(pointer.cast(), old_layout, new_layout) };
                if old_layout.align() < new_layout.align() {
                    if res.is_ok() {
                        return false;
                    }
                } else {
                    pointer = res.unwrap();
                    if !is_pointer_aligned_to(pointer, new_layout.align()) {
                        return false;
                    }

                    old_layout = new_layout;
                }
            }
        }

        true
    }
}

#[test]
fn allocator_shrink_layout_change() {
    let b = AllocatorDebug::new(Bump::with_capacity(1024));

    let layout_align4 = Layout::from_size_align(1024, 4).unwrap();
    let layout_align16 = Layout::from_size_align(256, 16).unwrap();

    // Allocate a chunk of memory and attempt to shrink it while increasing
    // alignment requirements.
    let p4: NonNull<u8> = b.allocate(layout_align4).unwrap().cast();
    let p16_res = unsafe { b.shrink(p4, layout_align4, layout_align16) };

    assert_eq!(p16_res, Err(AllocError));
}

fn is_pointer_aligned_to(p: NonNull<[u8]>, align: usize) -> bool {
    debug_assert!(align.is_power_of_two());

    let pointer = p.as_ptr() as *mut u8 as usize;
    let pointer_aligned = pointer & !(align - 1);

    pointer == pointer_aligned
}
