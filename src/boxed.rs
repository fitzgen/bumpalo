//! A pointer type for bump allocation.
//!
//! [`Box<'a, T>`], provides the simplest form of
//! bump allocation in `bumpalo`. Boxes provide ownership for this allocation, and
//! drop their contents when they go out of scope.
//!
//! # Examples
//!
//! Move a value from the stack to the heap by creating a [`Box`]:
//!
//! ```
//! use bumpalo::{Bump, boxed::Box};
//!
//! let b = Bump::new();
//!
//! let val: u8 = 5;
//! let boxed: Box<u8> = Box::new_in(val, &b);
//! ```
//!
//! Move a value from a [`Box`] back to the stack by [dereferencing]:
//!
//! ```
//! use bumpalo::{Bump, boxed::Box};
//!
//! let b = Bump::new();
//!
//! let boxed: Box<u8> = Box::new_in(5, &b);
//! let val: u8 = *boxed;
//! ```
//!
//! Creating a recursive data structure:
//!
//! ```
//! use bumpalo::{Bump, boxed::Box};
//!
//! let b = Bump::new();
//!
//! #[derive(Debug)]
//! enum List<'a, T> {
//!     Cons(T, Box<'a, List<'a, T>>),
//!     Nil,
//! }
//!
//! let list: List<i32> = List::Cons(1, Box::new_in(List::Cons(2, Box::new_in(List::Nil, &b)), &b));
//! println!("{:?}", list);
//! ```
//!
//! This will print `Cons(1, Cons(2, Nil))`.
//!
//! Recursive structures must be boxed, because if the definition of `Cons`
//! looked like this:
//!
//! ```compile_fail,E0072
//! # enum List<T> {
//! Cons(T, List<T>),
//! # }
//! ```
//!
//! It wouldn't work. This is because the size of a `List` depends on how many
//! elements are in the list, and so we don't know how much memory to allocate
//! for a `Cons`. By introducing a [`Box<T>`], which has a defined size, we know how
//! big `Cons` needs to be.
//!
//! # Memory layout
//!
//! For non-zero-sized values, a [`Box`] will use the provided [`Bump`] allocator for
//! its allocation. It is valid to convert both ways between a [`Box`] and a
//! pointer allocated with the [`Bump`] allocator, given that the
//! [`Layout`] used with the allocator is correct for the type. More precisely,
//! a `value: *mut T` that has been allocated with the [`Bump`] allocator
//! with `Layout::for_value(&*value)` may be converted into a box using
//! [`Box::<T>::from_raw(value)`]. Conversely, the memory backing a `value: *mut
//! T` obtained from [`Box::<T>::into_raw`] will be deallocated by the
//! [`Bump`] allocator with [`Layout::for_value(&*value)`].
//!
//! Note that roundtrip `Box::from_raw(Box::into_raw(b))` looses lifetime bound to the
//! [`Bump`] immutable borrow which guarantees that allocator will not be reset
//! and memory will not be freed.
//!
//! [dereferencing]: https://doc.rust-lang.org/std/ops/trait.Deref.html
//! [`Box`]: struct.Box.html
//! [`Box<T>`]: struct.Box.html
//! [`Box::<T>::from_raw(value)`]: struct.Box.html#method.from_raw
//! [`Box::<T>::into_raw`]: struct.Box.html#method.into_raw
//! [`Bump`]: ../struct.Bump.html
//! [`Layout`]: https://doc.rust-lang.org/std/alloc/struct.Layout.html
//! [`Layout::for_value(&*value)`]: https://doc.rust-lang.org/std/alloc/struct.Layout.html#method.for_value

use {
    crate::Bump,
    {
        core::{
            any::Any,
            borrow,
            cmp::Ordering,
            future::Future,
            hash::{Hash, Hasher},
            iter::FusedIterator,
            mem,
            ops::{Deref, DerefMut},
            pin::Pin,
            task::{Context, Poll},
        },
        core_alloc::fmt,
    },
};

/// A pointer type for bump allocation.
///
/// See the [module-level documentation](../../boxed/index.html) for more.
#[repr(transparent)]
pub struct Box<'a, T: ?Sized>(&'a mut T);

impl<'a, T> Box<'a, T> {
    /// Allocates memory on the heap and then places `x` into it.
    ///
    /// This doesn't actually allocate if `T` is zero-sized.
    ///
    /// # Examples
    ///
    /// ```
    /// use bumpalo::{Bump, boxed::Box};
    ///
    /// let b = Bump::new();
    ///
    /// let five = Box::new_in(5, &b);
    /// ```
    #[inline(always)]
    pub fn new_in(x: T, a: &'a Bump) -> Box<'a, T> {
        Box(a.alloc(x))
    }

    /// Constructs a new box with uninitialized contents.
    ///
    /// # Examples
    ///
    /// ```
    /// use bumpalo::{Bump, boxed::Box};
    ///
    /// let b = Bump::new();
    ///
    /// let mut five = Box::<u32>::new_uninit_in(&b);
    ///
    /// let five = unsafe {
    ///     // Deferred initialization:
    ///     five.as_mut_ptr().write(5);
    ///
    ///     five.assume_init()
    /// };
    ///
    /// assert_eq!(*five, 5)
    /// ```
    pub fn new_uninit_in(a: &'a Bump) -> Box<'a, mem::MaybeUninit<T>> {
        Box(a.alloc_with(|| mem::MaybeUninit::uninit()))
    }

    /// Constructs a new `Box` with uninitialized contents, with the memory
    /// being filled with `0` bytes.
    ///
    /// See [`MaybeUninit::zeroed`][zeroed] for examples of correct and incorrect usage
    /// of this method.
    ///
    /// # Examples
    ///
    /// ```
    /// use bumpalo::{Bump, boxed::Box};
    ///
    /// let b = Bump::new();
    ///
    /// let zero = Box::<u32>::new_zeroed_in(&b);
    /// let zero = unsafe { zero.assume_init() };
    ///
    /// assert_eq!(*zero, 0)
    /// ```
    ///
    /// [zeroed]: https://doc.rust-lang.org/std/mem/union.MaybeUninit.html#method.zeroed
    pub fn new_zeroed_in(a: &'a Bump) -> Box<'a, mem::MaybeUninit<T>> {
        Box(a.alloc_with(|| mem::MaybeUninit::zeroed()))
    }

    /// Constructs a new `Pin<Box<T>>`. If `T` does not implement `Unpin`, then
    /// `x` will be pinned in memory and unable to be moved.
    #[inline(always)]
    pub fn pin_in(x: T, a: &'a Bump) -> Pin<Box<'a, T>> {
        Box(a.alloc(x)).into()
    }
}

impl<'a, T> Box<'a, [T]> {
    /// Constructs a new boxed slice with uninitialized contents.
    ///
    /// # Examples
    ///
    /// ```
    /// use bumpalo::{Bump, boxed::Box};
    ///
    /// let b = Bump::new();
    ///
    /// let mut values = Box::<[u32]>::new_uninit_slice_in(3, &b);
    ///
    /// let values = unsafe {
    ///     // Deferred initialization:
    ///     values[0].as_mut_ptr().write(1);
    ///     values[1].as_mut_ptr().write(2);
    ///     values[2].as_mut_ptr().write(3);
    ///
    ///     values.assume_init()
    /// };
    ///
    /// assert_eq!(*values, [1, 2, 3])
    /// ```
    pub fn new_uninit_slice_in(len: usize, a: &'a Bump) -> Box<'a, [mem::MaybeUninit<T>]> {
        Box(a.alloc_slice_fill_with(len, |_| mem::MaybeUninit::uninit()))
    }
}

impl<'a, T> Box<'a, mem::MaybeUninit<T>> {
    /// Converts to `Box<T>`.
    ///
    /// # Safety
    ///
    /// As with [`MaybeUninit::assume_init`],
    /// it is up to the caller to guarantee that the value
    /// really is in an initialized state.
    /// Calling this when the content is not yet fully initialized
    /// causes immediate undefined behavior.
    ///
    /// [`MaybeUninit::assume_init`]: https://doc.rust-lang.org/std/mem/union.MaybeUninit.html#method.assume_init
    ///
    /// # Examples
    ///
    /// ```
    /// use bumpalo::{Bump, boxed::Box};
    ///
    /// let b = Bump::new();
    ///
    /// let mut five = Box::<u32>::new_uninit_in(&b);
    ///
    /// let five: Box<u32> = unsafe {
    ///     // Deferred initialization:
    ///     five.as_mut_ptr().write(5);
    ///
    ///     five.assume_init()
    /// };
    ///
    /// assert_eq!(*five, 5)
    /// ```
    #[inline]
    pub unsafe fn assume_init(self) -> Box<'a, T> {
        Box::from_raw(Box::into_raw(self) as *mut T)
    }
}

impl<'a, T> Box<'a, [mem::MaybeUninit<T>]> {
    /// Converts to `Box<[T]>`.
    ///
    /// # Safety
    ///
    /// As with [`MaybeUninit::assume_init`],
    /// it is up to the caller to guarantee that the values
    /// really are in an initialized state.
    /// Calling this when the content is not yet fully initialized
    /// causes immediate undefined behavior.
    ///
    /// [`MaybeUninit::assume_init`]: https://doc.rust-lang.org/std/mem/union.MaybeUninit.html#method.assume_init
    ///
    /// # Examples
    ///
    /// ```
    /// use bumpalo::{Bump, boxed::Box};
    ///
    /// let b = Bump::new();
    ///
    /// let mut values = Box::<[u32]>::new_uninit_slice_in(3, &b);
    ///
    /// let values = unsafe {
    ///     // Deferred initialization:
    ///     values[0].as_mut_ptr().write(1);
    ///     values[1].as_mut_ptr().write(2);
    ///     values[2].as_mut_ptr().write(3);
    ///
    ///     values.assume_init()
    /// };
    ///
    /// assert_eq!(*values, [1, 2, 3])
    /// ```
    #[inline]
    pub unsafe fn assume_init(self) -> Box<'a, [T]> {
        Box::from_raw(Box::into_raw(self) as *mut [T])
    }
}

impl<'a, T: ?Sized> Box<'a, T> {
    /// Constructs a box from a raw pointer.
    ///
    /// After calling this function, the raw pointer is owned by the
    /// resulting `Box`. Specifically, the `Box` destructor will call
    /// the destructor of `T` and free the allocated memory. For this
    /// to be safe, the memory must have been allocated in accordance
    /// with the [memory layout] used by `Box` .
    ///
    /// # Safety
    ///
    /// This function is unsafe because improper use may lead to
    /// memory problems. For example, a double-free may occur if the
    /// function is called twice on the same raw pointer.
    ///
    /// # Examples
    /// Recreate a `Box` which was previously converted to a raw pointer
    /// using [`Box::into_raw`]:
    /// ```
    /// use bumpalo::{Bump, boxed::Box};
    ///
    /// let b = Bump::new();
    ///
    /// let x = Box::new_in(5, &b);
    /// let ptr = Box::into_raw(x);
    /// let x = unsafe { Box::from_raw(ptr) }; // Note that new `x`'s lifetime is unbound. It must be bound to the `b` immutable borrow before `b` is reset.
    /// ```
    /// Manually create a `Box` from scratch by using the bump allocator:
    /// ```
    /// use std::alloc::{alloc, Layout};
    /// use bumpalo::{Bump, boxed::Box};
    ///
    /// let b = Bump::new();
    ///
    /// unsafe {
    ///     let ptr = b.alloc_layout(Layout::new::<i32>()).as_ptr() as *mut i32;
    ///     *ptr = 5;
    ///     let x = Box::from_raw(ptr); // Note that `x`'s lifetime is unbound. It must be bound to the `b` immutable borrow before `b` is reset.
    /// }
    /// ```
    ///
    /// [memory layout]: https://doc.rust-lang.org/std/boxed/index.html#memory-layout
    /// [`Layout`]: https://doc.rust-lang.org/std/alloc/struct.Layout.html
    /// [`Box::into_raw`]: https://doc.rust-lang.org/std/boxed/struct.Box.html#method.into_raw
    #[inline]
    pub unsafe fn from_raw(raw: *mut T) -> Self {
        Box(&mut *raw)
    }

    /// Consumes the `Box`, returning a wrapped raw pointer.
    ///
    /// The pointer will be properly aligned and non-null.
    ///
    /// After calling this function, the caller is responsible for the
    /// value previously managed by the `Box`. In particular, the
    /// caller should properly destroy `T`. The easiest way to
    /// do this is to convert the raw pointer back into a `Box` with the
    /// [`Box::from_raw`] function, allowing the `Box` destructor to perform
    /// the cleanup.
    ///
    /// Note: this is an associated function, which means that you have
    /// to call it as `Box::into_raw(b)` instead of `b.into_raw()`. This
    /// is so that there is no conflict with a method on the inner type.
    ///
    /// # Examples
    /// Converting the raw pointer back into a `Box` with [`Box::from_raw`]
    /// for automatic cleanup:
    /// ```
    /// use bumpalo::{Bump, boxed::Box};
    ///
    /// let b = Bump::new();
    ///
    /// let x = Box::new_in(String::from("Hello"), &b);
    /// let ptr = Box::into_raw(x);
    /// let x = unsafe { Box::from_raw(ptr) }; // Note that new `x`'s lifetime is unbound. It must be bound to the `b` immutable borrow before `b` is reset.
    /// ```
    /// Manual cleanup by explicitly running the destructor:
    /// ```
    /// use std::ptr;
    /// use bumpalo::{Bump, boxed::Box};
    ///
    /// let b = Bump::new();
    ///
    /// let mut x = Box::new_in(String::from("Hello"), &b);
    /// let p = Box::into_raw(x);
    /// unsafe {
    ///     ptr::drop_in_place(p);
    /// }
    /// ```
    ///
    /// [memory layout]: index.html#memory-layout
    /// [`Box::from_raw`]: struct.Box.html#method.from_raw
    #[inline]
    pub fn into_raw(b: Box<'a, T>) -> *mut T {
        let ptr = b.0 as *mut T;
        mem::forget(b);
        ptr
    }

    /// Consumes and leaks the `Box`, returning a mutable reference,
    /// `&'a mut T`. Note that the type `T` must outlive the chosen lifetime
    /// `'a`. If the type has only static references, or none at all, then this
    /// may be chosen to be `'static`.
    ///
    /// This function is mainly useful for data that lives for the remainder of
    /// the program's life. Dropping the returned reference will cause a memory
    /// leak. If this is not acceptable, the reference should first be wrapped
    /// with the [`Box::from_raw`] function producing a `Box`. This `Box` can
    /// then be dropped which will properly destroy `T` and release the
    /// allocated memory.
    ///
    /// Note: this is an associated function, which means that you have
    /// to call it as `Box::leak(b)` instead of `b.leak()`. This
    /// is so that there is no conflict with a method on the inner type.
    ///
    /// [`Box::from_raw`]: struct.Box.html#method.from_raw
    ///
    /// # Examples
    ///
    /// Simple usage:
    ///
    /// ```
    /// use bumpalo::{Bump, boxed::Box, vec};
    ///
    /// let b = Bump::new();
    ///
    /// let x = Box::new_in(41, &b);
    /// let reference: &mut usize = Box::leak(x);
    /// *reference += 1;
    /// assert_eq!(*reference, 42);
    /// ```
    ///
    /// Unsized data:
    ///
    /// ```
    /// use bumpalo::{Bump, boxed::Box, vec};
    ///
    /// let b = Bump::new();
    ///
    /// let x = vec![in &b; 1, 2, 3].into_boxed_slice();
    /// let reference = Box::leak(x);
    /// reference[0] = 4;
    /// assert_eq!(*reference, [4, 2, 3]);
    /// ```
    #[inline]
    pub fn leak(b: Box<'a, T>) -> &'a mut T {
        unsafe { &mut *Box::into_raw(b) }
    }

    /// Converts a `Box<T>` into a `Pin<Box<T>>`
    ///
    /// This conversion does not allocate on the heap and happens in place.
    ///
    /// This is also available via [`From`].
    pub fn into_pin(boxed: Box<'a, T>) -> Pin<Box<'a, T>> {
        // It's not possible to move or replace the insides of a `Pin<Box<T>>`
        // when `T: !Unpin`,  so it's safe to pin it directly without any
        // additional requirements.
        unsafe { Pin::new_unchecked(boxed) }
    }
}

impl<'a, T: ?Sized> Drop for Box<'a, T> {
    fn drop(&mut self) {
        unsafe {
            // `Box` owns value of `T`, but not memory behind it.
            core::ptr::drop_in_place(self.0);
        }
    }
}

impl<'a, 'b, T: ?Sized + PartialEq> PartialEq<Box<'b, T>> for Box<'a, T> {
    #[inline]
    fn eq(&self, other: &Box<'b, T>) -> bool {
        PartialEq::eq(&**self, &**other)
    }
    #[inline]
    fn ne(&self, other: &Box<'b, T>) -> bool {
        PartialEq::ne(&**self, &**other)
    }
}

impl<'a, 'b, T: ?Sized + PartialOrd> PartialOrd<Box<'b, T>> for Box<'a, T> {
    #[inline]
    fn partial_cmp(&self, other: &Box<'b, T>) -> Option<Ordering> {
        PartialOrd::partial_cmp(&**self, &**other)
    }
    #[inline]
    fn lt(&self, other: &Box<'b, T>) -> bool {
        PartialOrd::lt(&**self, &**other)
    }
    #[inline]
    fn le(&self, other: &Box<'b, T>) -> bool {
        PartialOrd::le(&**self, &**other)
    }
    #[inline]
    fn ge(&self, other: &Box<'b, T>) -> bool {
        PartialOrd::ge(&**self, &**other)
    }
    #[inline]
    fn gt(&self, other: &Box<'b, T>) -> bool {
        PartialOrd::gt(&**self, &**other)
    }
}

impl<'a, T: ?Sized + Ord> Ord for Box<'a, T> {
    #[inline]
    fn cmp(&self, other: &Box<'a, T>) -> Ordering {
        Ord::cmp(&**self, &**other)
    }
}

impl<'a, T: ?Sized + Eq> Eq for Box<'a, T> {}

impl<'a, T: ?Sized + Hash> Hash for Box<'a, T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (**self).hash(state);
    }
}

impl<'a, T: ?Sized + Hasher> Hasher for Box<'a, T> {
    fn finish(&self) -> u64 {
        (**self).finish()
    }
    fn write(&mut self, bytes: &[u8]) {
        (**self).write(bytes)
    }
    fn write_u8(&mut self, i: u8) {
        (**self).write_u8(i)
    }
    fn write_u16(&mut self, i: u16) {
        (**self).write_u16(i)
    }
    fn write_u32(&mut self, i: u32) {
        (**self).write_u32(i)
    }
    fn write_u64(&mut self, i: u64) {
        (**self).write_u64(i)
    }
    fn write_u128(&mut self, i: u128) {
        (**self).write_u128(i)
    }
    fn write_usize(&mut self, i: usize) {
        (**self).write_usize(i)
    }
    fn write_i8(&mut self, i: i8) {
        (**self).write_i8(i)
    }
    fn write_i16(&mut self, i: i16) {
        (**self).write_i16(i)
    }
    fn write_i32(&mut self, i: i32) {
        (**self).write_i32(i)
    }
    fn write_i64(&mut self, i: i64) {
        (**self).write_i64(i)
    }
    fn write_i128(&mut self, i: i128) {
        (**self).write_i128(i)
    }
    fn write_isize(&mut self, i: isize) {
        (**self).write_isize(i)
    }
}

impl<'a, T: ?Sized> From<Box<'a, T>> for Pin<Box<'a, T>> {
    /// Converts a `Box<T>` into a `Pin<Box<T>>`
    ///
    /// This conversion does not allocate on the heap and happens in place.
    fn from(boxed: Box<'a, T>) -> Self {
        Box::into_pin(boxed)
    }
}

impl<'a> Box<'a, dyn Any> {
    #[inline]
    /// Attempt to downcast the box to a concrete type.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::any::Any;
    ///
    /// fn print_if_string(value: Box<dyn Any>) {
    ///     if let Ok(string) = value.downcast::<String>() {
    ///         println!("String ({}): {}", string.len(), string);
    ///     }
    /// }
    ///
    /// let my_string = "Hello World".to_string();
    /// print_if_string(Box::new(my_string));
    /// print_if_string(Box::new(0i8));
    /// ```
    pub fn downcast<T: Any>(self) -> Result<Box<'a, T>, Box<'a, dyn Any>> {
        if self.is::<T>() {
            unsafe {
                let raw: *mut dyn Any = Box::into_raw(self);
                Ok(Box::from_raw(raw as *mut T))
            }
        } else {
            Err(self)
        }
    }
}

impl<'a> Box<'a, dyn Any + Send> {
    #[inline]
    /// Attempt to downcast the box to a concrete type.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::any::Any;
    ///
    /// fn print_if_string(value: Box<dyn Any + Send>) {
    ///     if let Ok(string) = value.downcast::<String>() {
    ///         println!("String ({}): {}", string.len(), string);
    ///     }
    /// }
    ///
    /// let my_string = "Hello World".to_string();
    /// print_if_string(Box::new(my_string));
    /// print_if_string(Box::new(0i8));
    /// ```
    pub fn downcast<T: Any>(self) -> Result<Box<'a, T>, Box<'a, dyn Any + Send>> {
        if self.is::<T>() {
            unsafe {
                let raw: *mut (dyn Any + Send) = Box::into_raw(self);
                Ok(Box::from_raw(raw as *mut T))
            }
        } else {
            Err(self)
        }
    }
}

impl<'a, T: fmt::Display + ?Sized> fmt::Display for Box<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<'a, T: fmt::Debug + ?Sized> fmt::Debug for Box<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<'a, T: ?Sized> fmt::Pointer for Box<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // It's not possible to extract the inner Uniq directly from the Box,
        // instead we cast it to a *const which aliases the Unique
        let ptr: *const T = &**self;
        fmt::Pointer::fmt(&ptr, f)
    }
}

impl<'a, T: ?Sized> Deref for Box<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &*self.0
    }
}

impl<'a, T: ?Sized> DerefMut for Box<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.0
    }
}

impl<'a, I: Iterator + ?Sized> Iterator for Box<'a, I> {
    type Item = I::Item;
    fn next(&mut self) -> Option<I::Item> {
        (**self).next()
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (**self).size_hint()
    }
    fn nth(&mut self, n: usize) -> Option<I::Item> {
        (**self).nth(n)
    }
    fn last(self) -> Option<I::Item> {
        #[inline]
        fn some<T>(_: Option<T>, x: T) -> Option<T> {
            Some(x)
        }
        self.fold(None, some)
    }
}

impl<'a, I: DoubleEndedIterator + ?Sized> DoubleEndedIterator for Box<'a, I> {
    fn next_back(&mut self) -> Option<I::Item> {
        (**self).next_back()
    }
    fn nth_back(&mut self, n: usize) -> Option<I::Item> {
        (**self).nth_back(n)
    }
}
impl<'a, I: ExactSizeIterator + ?Sized> ExactSizeIterator for Box<'a, I> {
    fn len(&self) -> usize {
        (**self).len()
    }
}

impl<'a, I: FusedIterator + ?Sized> FusedIterator for Box<'a, I> {}

impl<'a, T: ?Sized> borrow::Borrow<T> for Box<'a, T> {
    fn borrow(&self) -> &T {
        &**self
    }
}

impl<'a, T: ?Sized> borrow::BorrowMut<T> for Box<'a, T> {
    fn borrow_mut(&mut self) -> &mut T {
        &mut **self
    }
}

impl<'a, T: ?Sized> AsRef<T> for Box<'a, T> {
    fn as_ref(&self) -> &T {
        &**self
    }
}

impl<'a, T: ?Sized> AsMut<T> for Box<'a, T> {
    fn as_mut(&mut self) -> &mut T {
        &mut **self
    }
}

impl<'a, T: ?Sized> Unpin for Box<'a, T> {}

impl<'a, F: ?Sized + Future + Unpin> Future for Box<'a, F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        F::poll(Pin::new(&mut *self), cx)
    }
}
