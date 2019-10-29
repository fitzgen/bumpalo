#![cfg(feature = "collections")]
use bumpalo::{collections::Vec, Bump};
use std::cell::Cell;
use std::mem;

#[test]
fn push_a_bunch_of_items() {
    let b = Bump::new();
    let mut v = Vec::new_in(&b);
    for x in 0..10_000 {
        v.push(x);
    }
}

#[test]
#[allow(clippy::cognitive_complexity)]
fn test_realloc() {
    let b = Bump::with_capacity(1000);
    let v1_data = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let v2_data = [41, 42, 43, 44, 45, 46, 47, 48, 49, 50];
    let mut v1 = bumpalo::vec![in &b; 1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let v1_ptr = v1.as_ptr();
    assert!(10 <= v1.capacity() && v1.capacity() < 20);

    // Shift the size up and down a few times and see that it stays in place
    v1.reserve(100);
    assert!(v1.capacity() >= 100);
    assert_eq!(v1.as_ptr(), v1_ptr);
    assert_eq!(v1, v1_data);

    v1.shrink_to_fit();
    assert!(10 <= v1.capacity() && v1.capacity() < 20);
    assert_eq!(v1.as_ptr(), v1_ptr);
    assert_eq!(v1, v1_data);

    v1.reserve(100);
    assert!(v1.capacity() >= 100);
    assert_eq!(v1.as_ptr(), v1_ptr);
    assert_eq!(v1, v1_data);

    v1.shrink_to_fit();
    assert!(10 <= v1.capacity() && v1.capacity() < 20);
    assert_eq!(v1.as_ptr(), v1_ptr);
    assert_eq!(v1, v1_data);

    // Allocate just after our buffer, to see that it blocks in-place expansion
    let mut v2 = bumpalo::vec![in &b; 41, 42, 43, 44, 45, 46, 47, 48, 49, 50];
    let v2_ptr = v2.as_ptr();
    assert!(10 <= v2.capacity() && v2.capacity() < 20);
    assert_eq!(unsafe { v1_ptr.add(v1.capacity()) }, v2_ptr);

    v1.reserve(100);
    assert!(v1.capacity() >= 100);
    assert_ne!(v1.as_ptr(), v1_ptr);
    assert_eq!(v1, v1_data);

    // Our chunk is now [old, dead v1] [current v2] [current v1]
    let v1_ptr = v1.as_ptr();
    assert_eq!(unsafe { v2_ptr.add(v2.capacity()) }, v1_ptr);

    // See that we can still shrink at the new location as well
    v1.shrink_to_fit();
    assert!(10 <= v1.capacity() && v1.capacity() < 20);
    assert_eq!(v1.as_ptr(), v1_ptr);
    assert_eq!(v1, v1_data);

    // And see that we get a new chunk if we expand too much
    v1.reserve(10_000);
    assert!(v1.capacity() >= 10_000);
    assert_ne!(v1.as_ptr(), v1_ptr);
    assert_eq!(v1, v1_data);

    // See that we can deallocate and re-use the existing memory
    let v1_ptr = v1.as_ptr();
    mem::drop(v1);

    let mut v1 = bumpalo::vec![in &b; 1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    assert!(10 <= v1.capacity() && v1.capacity() < 20);
    assert_eq!(v1.as_ptr(), v1_ptr);

    // At this point, the old chunk is [old, dead v1] [current v2] [old, dead v1]
    // See that we can still shrink buffers that are not at the end without moving them
    v2.truncate(5);
    v2.shrink_to_fit();
    assert!(v2.capacity() < 10);
    assert_eq!(v2.as_ptr(), v2_ptr);
    assert_eq!(v2, &v2_data[..5]);

    // However we cannot increase their size back up again without moving
    v2.extend_from_slice(&[46, 47, 48, 49, 50]);
    assert!(10 <= v2.capacity() && v2.capacity() < 20);
    assert_ne!(v2.as_ptr(), v2_ptr);
    assert_eq!(v2, v2_data);
    let v2_ptr = v2.as_ptr();

    // At this point, our new chunk should be [current v1][current v2]
    assert_eq!(unsafe { v1_ptr.add(v1.capacity()) }, v2_ptr);

    // If we free v2, we should be able to extend v1 inplace
    mem::drop(v2);
    v1.reserve(100);
    assert!(v1.capacity() >= 100);
    assert_eq!(v1.as_ptr(), v1_ptr);
}

#[test]
fn recursive_vecs() {
    // The purpose of this test is to see if the data structures with
    // self references are allowed without causing a compile error
    // because of the dropck
    let b = Bump::new();

    struct Node<'a> {
        myself: Cell<Option<&'a Node<'a>>>,
        edges: Cell<Vec<'a, &'a Node<'a>>>,
    }

    let node1: &Node = b.alloc(Node {
        myself: Cell::new(None),
        edges: Cell::new(Vec::new_in(&b)),
    });
    let node2: &Node = b.alloc(Node {
        myself: Cell::new(None),
        edges: Cell::new(Vec::new_in(&b)),
    });

    node1.myself.set(Some(node1));
    node1.edges.set(bumpalo::vec![in &b; node1, node1, node2]);

    node2.myself.set(Some(node2));
    node2.edges.set(bumpalo::vec![in &b; node1, node2]);
}
