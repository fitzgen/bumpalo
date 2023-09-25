#![cfg(feature = "boxed")]

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
