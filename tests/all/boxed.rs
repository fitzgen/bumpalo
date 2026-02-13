#![cfg(feature = "boxed")]

use std::str::FromStr;

use bumpalo::boxed::Box;
use bumpalo::Bump;

#[test]
fn into_raw_aliasing() {
    let bump = Bump::new();
    let boxed = Box::new_in(1, &bump);
    let raw = Box::into_raw(boxed);

    let mut_ref = unsafe { &mut *raw };
    dbg!(mut_ref);
}

//This tests some basic functionality of the box.
#[test]
fn test_box_basic() {
    let bump = Bump::new();
    let mut value = Box::new_in("hello".to_string(), &bump);
    assert_eq!("hello", &*value);
    *value = "world".to_string();
    assert_eq!("world", &*value);
}

//If you change the PhantomData<&'a T> in Box to PhantomData<&'a mut T> the test fails the borrow checker.
#[allow(dead_code)]
#[derive(Debug)]
enum ValueBoxed<'a> {
    Str(&'a str),
    Wrapped(Box<'a, ValueBoxed<'a>>),
}
#[test]
fn test_box_covariant() {
    fn borrows_box<'a>(_input: &'a ValueBoxed<'a>) {}

    let bump = Bump::new();
    let mut nested = ValueBoxed::Str(bump.alloc_str("hello"));
    borrows_box(&mut nested);
    borrows_box(&mut nested);
}
