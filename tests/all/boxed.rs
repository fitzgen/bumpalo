#![cfg(feature = "boxed")]

use bumpalo::Bump;
use bumpalo::boxed::Box;

#[test]
fn into_raw_aliasing() {
    let bump = Bump::new();
    let boxed = Box::new_in(1, &bump);
    let raw = Box::into_raw(boxed);

    let mut_ref = unsafe { &mut *raw };
    dbg!(mut_ref);
}

// This tests some basic functionality of the box.
#[test]
fn test_box_basic() {
    let bump = Bump::new();
    let mut value = Box::new_in("hello".to_string(), &bump);
    assert_eq!("hello", &*value);
    *value = "world".to_string();
    assert_eq!("world", &*value);
}

// This function tests that `Box` is covariant.
fn _box_is_covariant<'sup, 'sub: 'sup>(
    a: Box<&'sup u32>,
    b: Box<&'sub u32>,
    f: impl Fn(Box<&'sup u32>),
) {
    f(a);
    f(b);
}
