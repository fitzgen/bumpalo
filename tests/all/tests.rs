use bumpalo::Bump;
use std::alloc::Layout;
use std::fmt::Debug;
use std::mem;
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

// Demonstrates a memory leak in `alloc_try_with` when the `Result` is allocated
// in a freshly created chunk and the closure returns `Err`.
//
// The rewind path on the new-chunk branch sets the bump pointer to
// `footer.data` (the LOWEST address of the chunk). Because the bump pointer
// grows downward (from footer toward data), setting it to `data` makes
// `chunk_capacity()` (= ptr - data) equal to zero — i.e. the chunk appears
// completely full. The intent is the opposite: the chunk is empty and should
// be fully reusable.
//
// Expected behavior: after the failing `alloc_try_with` call, the freshly
// allocated chunk should be empty (full capacity available).
// Actual behavior: the chunk reports zero capacity remaining; subsequent
// allocations are forced to allocate yet another new chunk, leaking the
// memory of the chunk allocated for the failed result.
#[test]
fn alloc_try_with_new_chunk_leak() {
    // Type whose size exceeds the bump's initial chunk capacity to force
    // `alloc_try_with` to allocate a new chunk for `Result<T, E>`.
    type Big = [u8; 4096];

    // Start with a small bump that does not have room for `Result<Big, ()>`.
    let bump = Bump::with_capacity(64);

    // Tiny allocation in the existing chunk so the rewind footer differs from
    // the new chunk that `alloc_try_with` will allocate.
    let _x = bump.alloc(1u8);

    let bytes_before = bump.allocated_bytes_including_metadata();

    // Trigger the failing path: closure returns Err.
    let res: Result<&mut Big, ()> = bump.alloc_try_with(|| Err(()));
    assert!(res.is_err());

    // After the failed call, the most recent chunk (newly allocated for the
    // result) should have its full capacity available, since the result was
    // the only allocation in that chunk and we rewound. But due to the bug,
    // `chunk_capacity()` is 0.
    let cap_after = bump.chunk_capacity();
    assert!(
        cap_after >= core::mem::size_of::<Big>(),
        "chunk_capacity() after failed alloc_try_with is {cap_after}, expected >= {}; the new chunk has been wrongly marked as full (memory leak)",
        core::mem::size_of::<Big>()
    );

    // Verify that a subsequent allocation of the same size doesn't require
    // yet another chunk to be allocated.
    let _y: &mut Big = bump.alloc([0u8; 4096]);
    let bytes_after = bump.allocated_bytes_including_metadata();

    // The Big allocation already has a chunk reserved for it (the one
    // allocated by alloc_try_with). If the rewind worked correctly, the
    // delta should equal roughly `size_of::<Big>()` (plus possibly small
    // alignment padding). With the bug, an additional chunk is allocated,
    // which roughly doubles the delta.
    let delta = bytes_after - bytes_before;
    assert!(
        delta < 2 * core::mem::size_of::<Big>(),
        "after failed alloc_try_with and a follow-up Big allocation, allocated_bytes_including_metadata grew by {delta}, expected close to {} (single chunk reuse). The new chunk allocated for the failed `alloc_try_with` has been leaked.",
        core::mem::size_of::<Big>()
    );
}

// Same issue for `try_alloc_try_with`.
#[test]
fn try_alloc_try_with_new_chunk_leak() {
    type Big = [u8; 4096];

    let bump = Bump::with_capacity(64);

    let _x = bump.alloc(1u8);
    let bytes_before = bump.allocated_bytes_including_metadata();

    let res: Result<&mut Big, bumpalo::AllocOrInitError<()>> = bump.try_alloc_try_with(|| Err(()));
    assert!(res.is_err());

    let cap_after = bump.chunk_capacity();
    assert!(
        cap_after >= core::mem::size_of::<Big>(),
        "chunk_capacity() after failed try_alloc_try_with is {cap_after}, expected >= {}; the new chunk has been wrongly marked as full (memory leak)",
        core::mem::size_of::<Big>()
    );

    let _y: &mut Big = bump.alloc([0u8; 4096]);
    let bytes_after = bump.allocated_bytes_including_metadata();

    let delta = bytes_after - bytes_before;
    assert!(
        delta < 2 * core::mem::size_of::<Big>(),
        "after failed try_alloc_try_with and a follow-up Big allocation, allocated_bytes_including_metadata grew by {delta}, expected close to {} (single chunk reuse). The new chunk allocated for the failed `try_alloc_try_with` has been leaked.",
        core::mem::size_of::<Big>()
    );
}
