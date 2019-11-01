#![cfg(feature = "collections")]
use bumpalo::{collections::Vec, Bump};
use std::cell::Cell;

#[test]
fn push_a_bunch_of_items() {
    let b = Bump::new();
    let mut v = Vec::new_in(&b);
    for x in 0..10_000 {
        v.push(x);
    }
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
