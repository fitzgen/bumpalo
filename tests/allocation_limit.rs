use std::alloc::Layout;

use bumpalo::Bump;
use quickcheck::quickcheck;

#[test]
fn allocation_limit_trivial() {
    let bump = Bump::with_capacity(0);
    bump.set_allocation_limit(0);

    assert!(bump.try_alloc(5).is_err());
    assert!(bump.allocation_limit().unwrap() >= bump.allocated_bytes());

    bump.remove_allocation_limit();

    assert!(bump.try_alloc(5).is_ok());
}

#[test]
fn change_allocation_limit_with_live_allocations() {
    let bump = Bump::new();

    bump.set_allocation_limit(512);

    bump.alloc(10);

    assert!(bump.try_alloc([0; 2048]).is_err());

    bump.set_allocation_limit(16384);

    assert!(bump.try_alloc([0; 2048]).is_ok());
    assert!(bump.allocation_limit().unwrap() >= bump.allocated_bytes());
}

#[test]
fn remove_allocation_limit_with_live_allocations() {
    let bump = Bump::new();

    bump.set_allocation_limit(512);

    bump.alloc(10);

    assert!(bump.try_alloc([0; 2048]).is_err());
    assert!(bump.allocation_limit().unwrap() >= bump.allocated_bytes());

    bump.remove_allocation_limit();

    assert!(bump.try_alloc([0; 2048]).is_ok());
}

#[test]
fn reset_preserves_allocation_limits() {
    let mut bump = Bump::new();

    bump.set_allocation_limit(512);
    bump.reset();

    assert!(bump.try_alloc([0; 2048]).is_err());
    assert!(bump.allocation_limit().unwrap() >= bump.allocated_bytes());
}

quickcheck! {
    fn limit_is_never_exceeded(xs: usize) -> bool {
        let b = Bump::new();

        b.set_allocation_limit(xs);

        // The exact numbers here on how much to allocate are a bit murky but we
        // have two main goals.
        //
        // - Attempt to allocate over the allocation limit imposed
        // - Allocate in increments small enough that at least a few allocations succeed
        let layout = Layout::array::<u8>(xs / 16).unwrap();
        for _ in 0..32 {
            let _ = b.try_alloc_layout(layout);
        }

        b.allocation_limit().unwrap() >= b.allocated_bytes()
    }
}
