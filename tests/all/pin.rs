#![cfg(feature = "pin")]

use core::future::Future;
use std::{
    mem,
    pin::Pin,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll, Wake, Waker},
};

use bumpalo::pin::Box;
use bumpalo::Bump;

struct NoopWaker;

impl Wake for NoopWaker {
    fn wake(self: std::sync::Arc<Self>) {}
}

#[test]
fn box_pin() {
    let bump = Bump::new();
    let mut fut = Box::pin_in(async { 1 }, &bump);
    let fut = fut.as_mut();

    let waker = Waker::from(Arc::new(NoopWaker));
    let mut context = Context::from_waker(&waker);

    assert_eq!(fut.poll(&mut context), Poll::Ready(1));
}

#[test]
fn dyn_box_pin() {
    struct Foo(Rc<AtomicBool>);
    impl Drop for Foo {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    let bump = Bump::new();
    let dropped = Rc::new(AtomicBool::new(false));

    // --

    let foo = Foo(dropped.clone());
    let fut = Box::new_in(
        async move {
            mem::forget(foo);
        },
        &bump,
    );
    let fut: Box<'_, dyn Future<Output = ()>> = fut.into();
    drop(fut);

    assert_eq!(dropped.load(Ordering::SeqCst), true);

    // --

    dropped.store(false, Ordering::SeqCst);

    let fut = Box::new_in(async move { 1 }, &bump);
    let fut: Box<'_, dyn Future<Output = usize>> = fut.into();
    let mut fut: Pin<Box<'_, dyn Future<Output = usize>>> = fut.into();
    let fut = fut.as_mut();

    let waker = Waker::from(Arc::new(NoopWaker));
    let mut context = Context::from_waker(&waker);

    assert_eq!(fut.poll(&mut context), Poll::Ready(1));
}

#[test]
fn box_pin_drop() {
    struct Foo(Rc<AtomicBool>);
    impl Drop for Foo {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    let dropped = Rc::new(AtomicBool::new(false));

    let bump = Bump::new();
    let foo = Box::pin_in(Foo(dropped.clone()), &bump);
    drop(foo);

    assert!(dropped.load(Ordering::SeqCst));
}

#[test]
fn box_pin_mut_drop() {
    struct Foo(Rc<AtomicBool>, String);
    impl Drop for Foo {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    let dropped = Rc::new(AtomicBool::new(false));

    let bump = Bump::new();
    let mut foo = Box::pin_in(Foo(dropped.clone(), String::new()), &bump);
    foo.1.push_str("123");

    drop(foo);

    assert!(dropped.load(Ordering::SeqCst));
}

#[test]
fn box_pin_forget_drop() {
    struct Foo(Rc<AtomicBool>);
    impl Drop for Foo {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    let dropped = Rc::new(AtomicBool::new(false));

    let bump = Bump::new();
    mem::forget(Box::pin_in(Foo(dropped.clone()), &bump));
    assert!(!dropped.load(Ordering::SeqCst));

    drop(bump);

    assert!(dropped.load(Ordering::SeqCst));
}

#[test]
fn box_pin_multiple_forget_drop() {
    struct Foo(Rc<AtomicUsize>);
    impl Drop for Foo {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    let dropped = Rc::new(AtomicUsize::new(0));

    let bump = Bump::new();
    mem::forget(Box::pin_in(Foo(dropped.clone()), &bump));
    mem::forget(Box::pin_in(Foo(dropped.clone()), &bump));
    mem::drop(Box::pin_in(Foo(dropped.clone()), &bump));
    mem::forget(Box::pin_in(Foo(dropped.clone()), &bump));

    assert_eq!(dropped.load(Ordering::SeqCst), 1);

    drop(bump);

    assert_eq!(dropped.load(Ordering::SeqCst), 4);
}

#[test]
fn box_pin_raw() {
    struct Foo(Rc<AtomicBool>, String);
    impl Drop for Foo {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    let dropped = Rc::new(AtomicBool::new(false));

    let bump = Bump::new();
    let foo = Pin::into_inner(Box::pin_in(Foo(dropped.clone(), String::new()), &bump));
    let ptr = Box::into_raw(foo);

    unsafe { (*ptr).1.push_str("Hello World") };

    let foo = unsafe { Box::from_raw(ptr, &bump) };
    drop(foo);

    assert!(dropped.load(Ordering::SeqCst));
}

#[test]
fn into_raw_aliasing() {
    let bump = Bump::new();
    let boxed = Box::new_in(1, &bump);
    let raw = Box::into_raw(boxed);

    let mut_ref = unsafe { &mut *raw };
    dbg!(mut_ref);
}
