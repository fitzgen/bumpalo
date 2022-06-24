use std::alloc::Layout;

use bumpalo::Bump;
use quickcheck::quickcheck;

#[test]
fn allocation_limit_trivial() {
    let bump = Bump::with_capacity(0);
    bump.set_allocation_limit(Some(0));

    assert!(bump.try_alloc(5).is_err());
    assert!(bump.allocation_limit().unwrap() >= bump.allocated_bytes());

    bump.set_allocation_limit(None);

    assert!(bump.try_alloc(5).is_ok());
}

#[test]
fn change_allocation_limit_with_live_allocations() {
    let bump = Bump::new();

    bump.set_allocation_limit(Some(512));

    bump.alloc(10);

    assert!(bump.try_alloc([0; 2048]).is_err());

    bump.set_allocation_limit(Some(16384));

    assert!(bump.try_alloc([0; 2048]).is_ok());
    assert!(bump.allocation_limit().unwrap() >= bump.allocated_bytes());
}

#[test]
fn remove_allocation_limit_with_live_allocations() {
    let bump = Bump::new();

    bump.set_allocation_limit(Some(512));

    bump.alloc(10);

    assert!(bump.try_alloc([0; 2048]).is_err());
    assert!(bump.allocation_limit().unwrap() >= bump.allocated_bytes());

    bump.set_allocation_limit(None);

    assert!(bump.try_alloc([0; 2048]).is_ok());
}

#[test]
fn reset_preserves_allocation_limits() {
    let mut bump = Bump::new();

    bump.set_allocation_limit(Some(512));
    bump.reset();

    assert!(bump.try_alloc([0; 2048]).is_err());
    assert!(bump.allocation_limit().unwrap() >= bump.allocated_bytes());
}

#[test]
fn reset_updates_allocated_bytes() {
    let mut bump = Bump::new();

    bump.alloc([0; 1 << 9]);

    // This second allocation should be a big enough one
    // after the first to force a new chunk allocation
    bump.alloc([0; 1 << 9]);

    let allocated_bytes_before_reset = bump.allocated_bytes();

    bump.reset();

    let allocated_bytes_after_reset = bump.allocated_bytes();

    assert!(allocated_bytes_after_reset < allocated_bytes_before_reset);
}

#[test]
fn new_bump_allocated_bytes_is_zero() {
    let bump = Bump::new();

    assert_eq!(bump.allocated_bytes(), 0);
}

quickcheck! {
    fn limit_is_never_exceeded(limit: usize) -> bool {
        let b = Bump::new();

        b.set_allocation_limit(Some(limit));

        // The exact numbers here on how much to allocate are a bit murky but we
        // have two main goals.
        //
        // - Attempt to allocate over the allocation limit imposed
        // - Allocate in increments small enough that at least a few allocations succeed
        let layout = Layout::array::<u8>(limit / 16).unwrap();
        for _ in 0..32 {
            let _ = b.try_alloc_layout(layout);
        }

        limit >= b.allocated_bytes()
    }
}
