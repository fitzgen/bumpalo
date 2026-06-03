use bumpalo::Bump;
use std::alloc::Layout;
use std::fmt::Debug;
use std::mem;
use std::ptr::NonNull;
use std::usize;

#[test]
fn can_iterate_over_allocated_things() {
    let mut bump = Bump::new();

    #[cfg(not(miri))]
    const MAX: u64 = 131_072;

    #[cfg(miri)] // Miri is very slow, pick a smaller max that runs in a reasonable amount of time
    const MAX: u64 = 1024;

    let mut chunk_ends = vec![];
    let mut last = None;

    for i in 0..MAX {
        let this = bump.alloc(i);
        assert_eq!(*this, i);
        let this = this as *const _ as usize;

        if match last {
            Some(last) if last - mem::size_of::<u64>() == this => false,
            _ => true,
        } {
            let chunk_end = this + mem::size_of::<u64>();
            println!("new chunk ending @ 0x{:x}", chunk_end);
            assert!(
                !chunk_ends.contains(&chunk_end),
                "should not have already allocated this chunk"
            );
            chunk_ends.push(chunk_end);
        }

        last = Some(this);
    }

    let mut seen = vec![false; MAX as usize];

    // Safe because we always allocated objects of the same type in this arena,
    // and their size >= their align.
    for ch in bump.iter_allocated_chunks() {
        let chunk_end = ch.as_ptr() as usize + ch.len();
        println!("iter chunk ending @ {:#x}", chunk_end);
        assert_eq!(
            chunk_ends.pop().unwrap(),
            chunk_end,
            "should iterate over each chunk once, in order they were allocated in"
        );

        let (before, mid, after) = unsafe { ch.align_to::<u64>() };
        assert!(before.is_empty());
        assert!(after.is_empty());
        for i in mid {
            assert!(*i < MAX, "{} < {} (aka {:x} < {:x})", i, MAX, i, MAX);
            seen[*i as usize] = true;
        }
    }

    assert!(seen.iter().all(|s| *s));
}

#[cfg(not(miri))] // Miri does not panic on OOM, the interpreter halts
#[test]
#[should_panic(expected = "out of memory")]
fn oom_instead_of_bump_pointer_overflow() {
    let bump = Bump::new();
    let x = bump.alloc(0_u8);
    let p = x as *mut u8 as usize;

    // Prevent bump from allocating a new chunk.
    bump.set_allocation_limit(Some(bump.allocated_bytes()));

    // A size guaranteed to overflow the bump pointer.
    // We assume that heap allocations are made in bottom half of address space, so `size < isize::MAX`.
    // If that assumption is incorrect, `Layout::from_size_align` will return `Err` and the test will fail.
    let size = p + 1;
    let align = 1;
    let layout = match Layout::from_size_align(size, align) {
        Err(e) => {
            // Return on error so that we don't panic and the test fails.
            eprintln!("Layout::from_size_align errored: {}", e);
            return;
        }
        Ok(l) => l,
    };

    // This should panic.
    bump.alloc_layout(layout);
}

#[test]
fn force_new_chunk_fits_well() {
    let b = Bump::new();

    // Use the first chunk for something
    b.alloc_layout(Layout::from_size_align(1, 1).unwrap());

    // Next force allocation of some new chunks.
    b.alloc_layout(Layout::from_size_align(100_001, 1).unwrap());
    b.alloc_layout(Layout::from_size_align(100_003, 1).unwrap());
}

#[test]
fn alloc_with_strong_alignment() {
    let b = Bump::new();

    // 64 is probably the strongest alignment we'll see in practice
    // e.g. AVX-512 types, or cache line padding optimizations
    b.alloc_layout(Layout::from_size_align(4096, 64).unwrap());
}

#[test]
fn alloc_slice_copy() {
    let b = Bump::new();

    let src: &[u16] = &[0xFEED, 0xFACE, 0xA7, 0xCAFE];
    let dst = b.alloc_slice_copy(src);

    assert_eq!(src, dst);
}

#[test]
fn alloc_slice_clone() {
    let b = Bump::new();

    let src = vec![vec![0], vec![1, 2], vec![3, 4, 5], vec![6, 7, 8, 9]];
    let dst = b.alloc_slice_clone(&src);

    assert_eq!(src, dst);
}

#[test]
fn small_size_and_large_align() {
    let b = Bump::new();
    let layout = std::alloc::Layout::from_size_align(1, 0x1000).unwrap();
    b.alloc_layout(layout);
}

fn with_capacity_helper<I, T>(iter: I)
where
    T: Copy + Debug + Eq,
    I: Clone + Iterator<Item = T> + DoubleEndedIterator,
{
    for &initial_size in &[0, 1, 8, 11, 0x1000, 0x12345] {
        let mut b = Bump::<1>::with_min_align_and_capacity(initial_size);

        for v in iter.clone() {
            b.alloc(v);
        }

        let mut pushed_values = b.iter_allocated_chunks().flat_map(|c| {
            let (before, mid, after) = unsafe { c.align_to::<T>() };
            assert!(before.is_empty());
            assert!(after.is_empty());
            mid.iter().copied()
        });

        let mut iter = iter.clone().rev();
        for (expected, actual) in iter.by_ref().zip(pushed_values.by_ref()) {
            assert_eq!(expected, actual);
        }

        assert!(iter.next().is_none());
        assert!(pushed_values.next().is_none());
    }
}

#[test]
fn with_capacity_test() {
    with_capacity_helper(0u8..255);
    #[cfg(not(miri))] // Miri is very slow, disable most of the test cases when using it
    {
        with_capacity_helper(0u16..10000);
        with_capacity_helper(0u32..10000);
        with_capacity_helper(0u64..10000);
        with_capacity_helper(0u128..10000);
    }
}

#[test]
fn test_reset() {
    let mut b = Bump::new();

    for i in 0u64..10_000 {
        b.alloc(i);
    }

    assert!(b.iter_allocated_chunks().count() > 1);

    let last_chunk = b.iter_allocated_chunks().next().unwrap();
    let start = last_chunk.as_ptr() as usize;
    let end = start + last_chunk.len();
    b.reset();
    assert_eq!(
        end - mem::size_of::<u64>(),
        b.alloc(0u64) as *const u64 as usize
    );
    assert_eq!(b.iter_allocated_chunks().count(), 1);
}

#[test]
fn test_alignment() {
    for &alignment in &[2, 4, 8, 16, 32, 64] {
        let b = Bump::with_capacity(513);
        let layout = std::alloc::Layout::from_size_align(alignment, alignment).unwrap();

        for _ in 0..1024 {
            let ptr = b.alloc_layout(layout).as_ptr();
            assert_eq!(ptr as *const u8 as usize % alignment, 0);
        }
    }
}

#[test]
fn test_chunk_capacity() {
    let b = Bump::with_capacity(512);
    let orig_capacity = b.chunk_capacity();
    b.alloc(true);
    assert!(b.chunk_capacity() < orig_capacity);
}

#[test]
#[cfg(feature = "allocator_api")]
fn miri_stacked_borrows_issue_247() {
    let bump = Bump::new();

    let (p, _) = Box::into_raw_with_allocator(Box::new_in(1u8, &bump));
    drop(unsafe { Box::from_raw_in(p, &bump) });

    let _q = Box::new_in(2u16, &bump);
}

#[test]
fn bump_is_send() {
    fn assert_send(_: impl Send) {}
    assert_send(Bump::new());
}

#[test]
fn test_debug_assert_data_le_bump_ptr_pr_313() {
    let bump = Bump::new();
    bump.set_allocation_limit(Some(1));
    bump.alloc_layout(Layout::from_size_align(0, 16).unwrap());
}

#[test]
fn test_debug_assert_ptr_align_pr_313() {
    let bump = Bump::<16>::with_min_align();
    bump.alloc(0u8);
}

#[test]
#[cfg(feature = "allocator_api")]
fn checkpoint_after_shrink_realloc() {
    use std::alloc::Allocator;

    let bump = Bump::new();
    let alloc: &Bump = &bump;

    // Make a large allocation.
    let layout_big = Layout::from_size_align(256, 1).unwrap();
    let ptr = alloc.allocate(layout_big).unwrap();
    unsafe {
        std::ptr::write_bytes(ptr.as_ptr().cast::<u8>(), 0xAA, 256);
    }

    // Take a checkpoint *after* the large allocation.
    let cp = bump.raw_checkpoint();

    // Shrink the allocation. This backs up the bump pointer past the
    // checkpoint's saved position.
    let layout_small = Layout::from_size_align(16, 1).unwrap();
    let shrunk = unsafe { alloc.shrink(ptr.cast(), layout_big, layout_small) }.unwrap();

    // Fill the shrunk region with a different byte pattern.
    unsafe {
        std::ptr::write_bytes(shrunk.as_ptr().cast::<u8>(), 0xBB, 16);
    }

    // Reset to the checkpoint. The bump pointer should not move backward
    // past its current position (which is already "above" the checkpoint).
    unsafe {
        bump.reset_to_raw_checkpoint(cp);
    }

    // Allocate again after the reset. This must not overlap with the
    // still-live shrunk allocation, nor trigger UB.
    let after = bump.alloc(42u8);
    assert_eq!(*after, 42);
}

#[test]
fn emplace_drop_after_intervening_alloc() {
    let bump = Bump::new();

    // Create an emplace that reserves space but don't finalize it.
    let place = bump.emplace::<u64>();

    // Make an intervening allocation.
    let x = bump.alloc(0x12345678u64);

    // Drop the emplace without finalizing. The inner `RewindGuard` fires but
    // its pointer is not the most recent allocation, so it cannot rewind
    // `bump`.
    drop(place);

    // The intervening allocation must still be valid.
    assert_eq!(*x, 0x12345678);
}

#[test]
#[cfg(feature = "allocator_api")]
fn emplace_drop_after_intervening_realloc() {
    use std::alloc::Allocator;

    let bump = Bump::new();
    let alloc: &Bump = &bump;

    // Make an initial allocation that we'll grow later.
    let layout_small = Layout::from_size_align(8, 8).unwrap();
    let ptr = alloc.allocate(layout_small).unwrap();
    unsafe {
        std::ptr::write_bytes(ptr.as_ptr().cast::<u8>(), 0xAA, 8);
    }

    // Create an emplace that reserves space but don't finalize it.
    let place = bump.emplace::<u64>();

    // Grow the earlier allocation. This makes a new allocation in the bump, so
    // the `RewindGuard`'s pointer is not the last allocation anymore.
    let layout_big = Layout::from_size_align(64, 8).unwrap();
    let grown = unsafe { alloc.grow(ptr.cast(), layout_small, layout_big) }.unwrap();
    unsafe {
        std::ptr::write_bytes(grown.as_ptr().cast::<u8>().offset(8), 0xBB, 64 - 8);
    }

    // Drop the emplace without finalizing. The inner `RewindGuard` fires but
    // its pointer is not the most recent allocation, so it cannot rewind
    // `bump`.
    drop(place);

    // The grown allocation must still be valid: first 8 bytes are 0xAA, rest
    // are 0xBB.
    let slice = unsafe { std::slice::from_raw_parts(grown.as_ptr().cast::<u8>(), 8) };
    assert!(slice.iter().all(|&b| b == 0xAA));
    let slice =
        unsafe { std::slice::from_raw_parts(grown.as_ptr().cast::<u8>().offset(8), 64 - 8) };
    assert!(slice.iter().all(|&b| b == 0xBB));
}

#[test]
fn emplace_drop_after_intervening_chunk_alloc() {
    let mut bump = Bump::with_capacity(64);

    let initial_chunks = bump.iter_allocated_chunks().count();

    let place = bump.emplace::<u64>();

    // Allocate `usize`s until the bump spills into a new chunk. Store raw
    // pointers so we don't hold a `&bump` borrow across the `drop(place)` and
    // `iter_allocated_chunks` calls below.
    let count = 256usize;
    let mut ptrs = vec![];
    for i in 0..count {
        let r: &mut usize = bump.alloc(i);
        let p: NonNull<usize> = NonNull::from(r);
        ptrs.push(p);
    }

    // Drop the emplace without finalizing. The inner `RewindGuard` fires but
    // its pointer is not the most recent allocation, so it cannot rewind
    // `bump`.
    drop(place);

    assert!(
        bump.iter_allocated_chunks().count() > initial_chunks,
        "should have allocated at least one new chunk"
    );

    // Our allocated values across multiple chunks should still be correct, and
    // not invalidated.
    for (i, &p) in ptrs.iter().enumerate() {
        assert_eq!(unsafe { *p.as_ref() }, i);
    }
}
