//! This tests integration with the `zerocopy` crate, which allows direct allocation of types
//! that can be safely constructed from an all-zeroes bit pattern.

use bumpalo::Bump;
use zerocopy_derive::FromZeroes;

#[repr(C)]
#[derive(FromZeroes)]
struct Foo {
    a: u32,
    b: u8,
    // <-- 3 bytes of padding
}

#[test]
fn alloc_zeroed() {
    let b = Bump::new();
    let f = b.alloc_zeroed::<Foo>();
    assert_eq!(f.a, 0);
    assert_eq!(f.b, 0);
}

#[test]
fn alloc_slice_zeroed() {
    let b = Bump::new();
    let s = b.alloc_slice_zeroed::<Foo>(10);
    assert_eq!(s.len(), 10);
    assert_eq!(s[0].a, 0);
    assert_eq!(s[9].a, 0);
}

#[test]
fn alloc_slice_zeroed_empty() {
    let b = Bump::new();
    let s = b.alloc_slice_zeroed::<Foo>(0);
    assert!(s.is_empty());
}

// Types outside of this crate also implement FromZeroes.
// bool implements FromZeroes, although it does _not_ implement FromBytes.
#[test]
fn alloc_slice_zeroed_external() {
    let b = Bump::new();
    let s = b.alloc_slice_zeroed::<bool>(10);
    assert_eq!(s.len(), 10);
    assert!(!s[0]);
}

#[repr(C)]
#[derive(FromZeroes)]
struct Empty {}

#[test]
fn alloc_zeroed_zst() {
    let b = Bump::new();
    b.alloc_zeroed::<Empty>();
}

#[test]
fn alloc_slice_zeroed_zst() {
    let b = Bump::new();
    let s = b.alloc_slice_zeroed::<Empty>(10);
    assert_eq!(s.len(), 10);
}

#[test]
fn alloc_slice_zeroed_zst_empty() {
    let b = Bump::new();
    let s = b.alloc_slice_zeroed::<Empty>(0);
    assert!(s.is_empty());
}

// Allocate a large object directly in a Bump. This avoids the problem of
// Box::new() for large types, where the type is constructed on the stack
// _before_ the heap allocation. This pattern (when using Box::new()) can
// cause stack overflow, but when using Bump::alloc_zeroed() the object is
// allocated directly in the heap.
#[test]
fn alloc_zeroed_big() {
    let b = Bump::new();
    let big = b.alloc_zeroed::<[u32; 0x10000]>();
    assert_eq!(big.len(), 0x10000);
}
