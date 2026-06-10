#![cfg(feature = "collections")]

use crate::check::check;
use bumpalo::{collections::Vec, vec, Bump};
use mutatis::check::CheckResult;
use mutatis::Mutate;
use std::cell::{Cell, RefCell};
use std::ops::Deref;

const MAX_VEC_OPS: usize = 64;
const MAX_VEC_SLICE_LEN: usize = 16;
const MAX_VEC_LEN: usize = 48;

#[derive(Clone, Debug, Mutate)]
enum VecOp {
    Push(u8),
    Pop,
    Extend(std::vec::Vec<u8>),
    Resize { len: u8, fill: u8 },
    Truncate(u8),
    ShrinkToFit,
}

#[derive(Clone, Debug, Default, Mutate)]
struct VecProgram(std::vec::Vec<VecOp>);

fn apply_vec_op(actual: &mut Vec<'_, u8>, expected: &mut std::vec::Vec<u8>, op: &VecOp) {
    match op {
        VecOp::Push(value) => {
            actual.push(*value);
            expected.push(*value);
        }
        VecOp::Pop => {
            assert_eq!(actual.pop(), expected.pop());
        }
        VecOp::Extend(values) => {
            let values = &values[..values.len().min(MAX_VEC_SLICE_LEN)];
            actual.extend_from_slice_copy(values);
            expected.extend_from_slice(values);
        }
        VecOp::Resize { len, fill } => {
            let len = usize::from(*len) % MAX_VEC_LEN;
            actual.resize(len, *fill);
            expected.resize(len, *fill);
        }
        VecOp::Truncate(len) => {
            let len = usize::from(*len) % MAX_VEC_LEN;
            actual.truncate(len);
            expected.truncate(len);
        }
        VecOp::ShrinkToFit => {
            actual.shrink_to_fit();
            expected.shrink_to_fit();
        }
    }

    assert_eq!(actual.as_slice(), expected.as_slice());
    assert!(actual.capacity() >= actual.len());
}

#[test]
fn push_a_bunch_of_items() {
    let b = Bump::new();
    let mut v = Vec::new_in(&b);
    for x in 0..10_000 {
        v.push(x);
    }
}

#[test]
fn trailing_comma_in_vec_macro() {
    let b = Bump::new();
    let v = vec![in &b; 1, 2, 3,];
    assert_eq!(v, [1, 2, 3]);
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

#[test]
fn test_into_bump_slice_mut() {
    let b = Bump::new();
    let v = bumpalo::vec![in &b; 1, 2, 3];
    let slice = v.into_bump_slice_mut();

    slice[0] = 3;
    slice[2] = 1;

    assert_eq!(slice, [3, 2, 1]);
}

#[test]
fn vec_resizes_causing_reallocs() -> CheckResult<std::vec::Vec<usize>> {
    check().run(|sizes: &std::vec::Vec<usize>| -> Result<(), String> {
        // Exercise `realloc` by doing a bunch of `resize`s followed by
        // `shrink_to_fit`s.

        let b = Bump::new();
        let mut v = bumpalo::vec![in &b];

        for len in sizes.iter().copied() {
            // We don't want to get too big and OOM.
            //
            // Under MIRI we cap this much lower because its so much slower, but
            // this smaller cap still comfortably exceeds the chunk size and
            // exercises cross-chunk reallocation.
            #[cfg(miri)]
            const MAX_SIZE: usize = 1 << 11;
            #[cfg(not(miri))]
            const MAX_SIZE: usize = 1 << 15;

            // But we want allocations to get fairly close to the minimum chunk
            // size, so that we are exercising both realloc'ing within a chunk
            // and when we need new chunks.
            const MIN_SIZE: usize = 1 << 7;

            let len = std::cmp::min(len, MAX_SIZE);
            let len = std::cmp::max(len, MIN_SIZE);

            v.resize(len, 0);
            v.shrink_to_fit();
        }
        Ok(())
    })
}

#[test]
fn vec_operation_sequences_match_std() -> CheckResult<VecProgram> {
    check().run(|program: &VecProgram| -> Result<(), String> {
        let bump = Bump::new();
        let mut actual = Vec::new_in(&bump);
        let mut expected = std::vec::Vec::new();

        for op in program.0.iter().take(MAX_VEC_OPS) {
            apply_vec_op(&mut actual, &mut expected, op);
        }

        assert_eq!(actual.into_bump_slice(), expected.as_slice());
        Ok(())
    })
}

#[test]
fn test_vec_items_get_dropped() {
    struct Foo<'a>(&'a RefCell<String>);
    impl<'a> Drop for Foo<'a> {
        fn drop(&mut self) {
            self.0.borrow_mut().push_str("Dropped!");
        }
    }

    let buffer = RefCell::new(String::new());
    let bump = Bump::new();
    {
        let mut vec_foo = Vec::new_in(&bump);
        vec_foo.push(Foo(&buffer));
        vec_foo.push(Foo(&buffer));
    }
    assert_eq!("Dropped!Dropped!", buffer.borrow().deref());
}

#[test]
fn test_extend_from_slice_copy() {
    let bump = Bump::new();
    let mut vec = vec![in &bump; 1, 2, 3];
    assert_eq!(&[1, 2, 3][..], vec.as_slice());

    vec.extend_from_slice_copy(&[4, 5, 6]);
    assert_eq!(&[1, 2, 3, 4, 5, 6][..], vec.as_slice());

    // Confirm that passing an empty slice is a no-op
    vec.extend_from_slice_copy(&[]);
    assert_eq!(&[1, 2, 3, 4, 5, 6][..], vec.as_slice());

    vec.extend_from_slice_copy(&[7]);
    assert_eq!(&[1, 2, 3, 4, 5, 6, 7][..], vec.as_slice());
}

#[test]
fn test_extend_from_slices_copy() {
    let bump = Bump::new();
    let mut vec = vec![in &bump; 1, 2, 3];
    assert_eq!(&[1, 2, 3][..], vec.as_slice());

    // Confirm that passing an empty slice of slices is a no-op
    vec.extend_from_slices_copy(&[]);
    assert_eq!(&[1, 2, 3][..], vec.as_slice());

    // Confirm that an empty slice in the slice-of-slices is a no-op
    vec.extend_from_slices_copy(&[&[4, 5, 6], &[], &[7]]);
    assert_eq!(&[1, 2, 3, 4, 5, 6, 7][..], vec.as_slice());

    vec.extend_from_slices_copy(&[&[8], &[9, 10, 11], &[12]]);
    assert_eq!(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12], vec.as_slice());
}

#[cfg(feature = "std")]
#[test]
fn test_vec_write() {
    use std::io::Write;

    let b = Bump::new();
    let mut v = bumpalo::vec![in &b];

    assert_eq!(v.write(&[]).unwrap(), 0);

    v.flush().unwrap();

    assert_eq!(v.write(&[1]).unwrap(), 1);

    v.flush().unwrap();

    v.write_all(&[]).unwrap();

    v.flush().unwrap();

    v.write_all(&[2, 3]).unwrap();

    v.flush().unwrap();

    assert_eq!(v, &[1, 2, 3]);
}

#[cfg(feature = "std")]
#[test]
fn panic_in_drain_filter() {
    use std::panic::AssertUnwindSafe;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);
    DROP_COUNT.store(0, Ordering::SeqCst);

    #[derive(Debug)]
    struct Tracked(i32);
    impl Drop for Tracked {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }

    let bump = Bump::new();

    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        let mut v: Vec<Tracked> = Vec::new_in(&bump);
        // Will be drained.
        v.push(Tracked(1));
        // Predicate will panic here.
        v.push(Tracked(2));
        // Will be kept.
        v.push(Tracked(3));

        let mut df = v.drain_filter(|x| {
            if x.0 == 1 {
                true
            } else if x.0 == 2 {
                panic!()
            } else {
                false
            }
        });

        // Drain `Tracked(1)`.
        let drained = df.next();
        assert!(drained.is_some());

        // Panics on `Tracked(2)`.
        let _ = df.next();
    }));
    assert!(result.is_err());

    let count = DROP_COUNT.load(Ordering::SeqCst);
    assert_eq!(count, 2);
}

#[cfg(feature = "std")]
#[test]
fn panic_in_retain() {
    use std::panic::AssertUnwindSafe;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);
    DROP_COUNT.store(0, Ordering::SeqCst);

    #[derive(Debug)]
    struct Tracked(i32);
    impl Drop for Tracked {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }

    let bump = Bump::new();

    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        let mut v: Vec<Tracked> = Vec::new_in(&bump);
        // Will be drained.
        v.push(Tracked(1));
        // Predicate will panic here.
        v.push(Tracked(2));
        // Will be kept.
        v.push(Tracked(3));

        v.retain(|x| {
            if x.0 == 1 {
                false
            } else if x.0 == 2 {
                panic!()
            } else {
                true
            }
        });
    }));
    assert!(result.is_err());

    let count = DROP_COUNT.load(Ordering::SeqCst);
    assert_eq!(count, 3);
}

#[cfg(feature = "std")]
#[test]
fn panic_in_retain_mut() {
    use std::panic::AssertUnwindSafe;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);
    DROP_COUNT.store(0, Ordering::SeqCst);

    #[derive(Debug)]
    struct Tracked(i32);
    impl Drop for Tracked {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }

    let bump = Bump::new();

    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        let mut v: Vec<Tracked> = Vec::new_in(&bump);
        // Will be drained.
        v.push(Tracked(1));
        // Predicate will panic here.
        v.push(Tracked(2));
        // Will be kept.
        v.push(Tracked(3));

        v.retain_mut(|x| {
            if x.0 == 1 {
                false
            } else if x.0 == 2 {
                panic!()
            } else {
                true
            }
        });
    }));
    assert!(result.is_err());

    let count = DROP_COUNT.load(Ordering::SeqCst);
    assert_eq!(count, 3);
}
