/*!

**A fast bump allocation arena for Rust.**

[![](https://docs.rs/bumpalo/badge.svg)](https://docs.rs/bumpalo/)
[![](https://img.shields.io/crates/v/bumpalo.svg)](https://crates.io/crates/bumpalo)
[![](https://img.shields.io/crates/d/bumpalo.svg)](https://crates.io/crates/bumpalo)
[![Travis CI Build Status](https://travis-ci.org/fitzgen/bumpalo.svg?branch=master)](https://travis-ci.org/fitzgen/bumpalo)

![](https://github.com/fitzgen/bumpalo/raw/master/bumpalo.png)

## Bump Allocation

Bump allocation is a fast, but limited approach to allocation. We have a chunk
of memory, and we maintain a pointer within that memory. Whenever we allocate an
object, we do a quick test that we have enough capacity left in our chunk to
allocate the object and then increment the pointer by the object's size. *That's
it!*

The disadvantage of bump allocation is that there is no general way to
deallocate individual objects or reclaim the memory region for a
no-longer-in-use object.

These trade offs make bump allocation well-suited for *phase-oriented*
allocations. That is, a group of objects that will all be allocated during the
same program phase, used, and then can all be deallocated together as a group.

## Deallocation en Masse, but No `Drop`

To deallocate all the objects in the arena at once, we can simply reset the bump
pointer back to the start of the arena's memory chunk. This makes mass
deallocation *extremely* fast, but allocated objects' `Drop` implementations are
not invoked.

See [the `BumpAllocSafe` marker
trait](https://docs.rs/bumpalo/1.0.2/bumpalo/trait.BumpAllocSafe.html) for
details.

## What happens when the memory chunk is full?

This implementation will allocate a new memory chunk from the global allocator
and then start bump allocating into this new memory chunk.

## Example

```
use bumpalo::{Bump, BumpAllocSafe};
use std::u64;

struct Doggo {
    cuteness: u64,
    age: u8,
    scritches_required: bool,
}

// Mark `Doggo` as safe to put into bump allocation arenas.
impl BumpAllocSafe for Doggo {}

// Create a new arena to bump allocate into.
let bump = Bump::new();

// Allocate values into the arena.
let scooter = bump.alloc(Doggo {
    cuteness: u64::max_value(),
    age: 8,
    scritches_required: true,
});

assert!(scooter.scritches_required);
```

## `#![no_std]` Support

Requires the `alloc` nightly feature. Disable the on-by-default `"std"` feature:

```toml
[dependencies.bumpalo]
version = "1"
default-features = false
```

 */

#![deny(missing_debug_implementations)]
#![deny(missing_docs)]
// In no-std mode, use the alloc crate to get `Vec`.
#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(not(feature = "std"), feature(alloc))]

extern crate core;

pub mod collections;

mod alloc;
mod impls;

#[cfg(feature = "std")]
mod imports {
    pub use std::alloc::{alloc, dealloc, Layout};
    pub use std::cell::{Cell, UnsafeCell};
    pub use std::fmt;
    pub use std::mem;
    pub use std::ptr::{self, NonNull};
    pub use std::slice;
}

#[cfg(not(feature = "std"))]
mod imports {
    extern crate alloc;
    pub use self::alloc::alloc::{alloc, dealloc, Layout};
    pub use core::cell::{Cell, UnsafeCell};
    pub use core::fmt;
    pub use core::mem;
    pub use core::ptr::{self, NonNull};
    pub use core::slice;
}

use imports::*;

/// A marker trait for types that are "safe" to bump alloc.
///
/// Objects that are bump-allocated will not have their `Drop` implementation
/// called, which makes it easy to leak memory or other resources. If you put
/// anything which heap allocates or manages open file descriptors or other
/// resources into a `Bump`, and that thing relies on its `Drop` implementation
/// to clean up after itself, then you need to find a new way to clean up after
/// it yourself. This could be calling
/// [`drop_in_place`](https://doc.rust-lang.org/stable/std/ptr/fn.drop_in_place.html),
/// or simply avoiding using such types in a `Bump`.
///
/// This is memory safe! Since destructors are never guaranteed to run in Rust,
/// you can't rely on them for enforcing memory safety. Therefore, implementing
/// this trait is **not** `unsafe` in the Rust sense (which is only about memory
/// safety). But instead of taking any `T`, bump allocation requires that you
/// implement this marker trait for `T` just so that you know what you're
/// getting into.
///
/// ## Example
///
/// ```
/// struct Point {
///     x: u64,
///     y: u64,
/// }
///
/// // We want to bump allocate `Point`s, so we implement
/// // `BumpAllocSafe` for them.
/// impl bumpalo::BumpAllocSafe for Point {}
/// ```
pub trait BumpAllocSafe {}

/// An arena to bump allocate into.
///
/// ## Example
///
/// ```
/// use bumpalo::Bump;
///
/// // Create a new bump arena.
/// let bump = Bump::new();
///
/// // Allocate values into the arena.
/// let forty_two = bump.alloc(42);
/// assert_eq!(*forty_two, 42);
///
/// // Mutable references are returned from allocation.
/// let mut s = bump.alloc("bumpalo");
/// *s = "the bump allocator; and also is a buffalo";
/// ```
#[derive(Debug)]
pub struct Bump {
    // The current chunk we are bump allocating within.
    current_chunk_footer: Cell<NonNull<ChunkFooter>>,

    // The first chunk we were ever given, which is the head of the intrusive
    // linked list of all chunks this arena has been bump allocating within.
    all_chunk_footers: Cell<NonNull<ChunkFooter>>,
}

#[repr(C)]
#[derive(Debug)]
struct ChunkFooter {
    // Pointer to the start of this chunk allocation. This footer is always at
    // the end of the chunk.
    data: NonNull<u8>,

    // Link to the next chunk, if any.
    next: Cell<Option<NonNull<ChunkFooter>>>,

    // Bump allocation finger that is always in the range `self.data..=self`.
    ptr: Cell<NonNull<u8>>,
}

impl Drop for Bump {
    fn drop(&mut self) {
        unsafe {
            let mut footer = Some(self.all_chunk_footers.get());
            while let Some(f) = footer {
                footer = f.as_ref().next.get();
                dealloc(f.as_ref().data.as_ptr(), Bump::chunk_layout());
            }
        }
    }
}

#[inline]
pub(crate) fn round_up_to(n: usize, divisor: usize) -> usize {
    debug_assert!(divisor.is_power_of_two());
    (n + divisor - 1) & !(divisor - 1)
}

// Maximum typical overhead per allocation imposed by allocators for
// wasm32-unknown-unknown.
const MALLOC_OVERHEAD: usize = 16;

const CHUNK_SIZE_WITH_FOOTER: usize = (1 << 16) - MALLOC_OVERHEAD;
const CHUNK_SIZE: usize = CHUNK_SIZE_WITH_FOOTER - mem::size_of::<ChunkFooter>();
const CHUNK_ALIGN: usize = 8;

impl Bump {
    #[inline]
    fn chunk_layout() -> Layout {
        if cfg!(debug_assertions) {
            Layout::from_size_align(CHUNK_SIZE_WITH_FOOTER, CHUNK_ALIGN).unwrap()
        } else {
            unsafe { Layout::from_size_align_unchecked(CHUNK_SIZE_WITH_FOOTER, CHUNK_ALIGN) }
        }
    }

    /// Construct a new arena to bump allocate into.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// # let _ = bump;
    /// ```
    pub fn new() -> Bump {
        let chunk_footer = Self::new_chunk();
        Bump {
            current_chunk_footer: Cell::new(chunk_footer),
            all_chunk_footers: Cell::new(chunk_footer),
        }
    }

    fn new_chunk() -> NonNull<ChunkFooter> {
        unsafe {
            let data = alloc(Bump::chunk_layout());
            assert!(!data.is_null());
            let data = NonNull::new_unchecked(data);

            let next = Cell::new(None);
            let ptr = Cell::new(data);
            let footer_ptr = data.as_ptr() as usize + CHUNK_SIZE;
            let footer_ptr = footer_ptr as *mut ChunkFooter;
            ptr::write(footer_ptr, ChunkFooter { data, next, ptr });
            NonNull::new_unchecked(footer_ptr)
        }
    }

    /// Reset this bump allocator.
    ///
    /// Performs mass deallocation on everything allocated in this arena by
    /// resetting the pointer into the underlying chunk of memory to the start
    /// of the chunk. Does not run any `Drop` implementations on deallocated
    /// objects; see [the `BumpAllocSafe` marker
    /// trait](./trait.BumpAllocSafe.html) for details.
    ///
    /// If this arena has allocated multiple chunks to bump allocate into, then
    /// the excess chunks are returned to the global allocator.
    ///
    /// ## Example
    ///
    /// ```
    /// let mut bump = bumpalo::Bump::new();
    ///
    /// // Allocate a bunch of things.
    /// {
    ///     for i in 0..100 {
    ///         bump.alloc(i);
    ///     }
    /// }
    ///
    /// // Reset the arena.
    /// bump.reset();
    ///
    /// // Allocate some new things in the space previously occupied by the
    /// // original things.
    /// for j in 200..400 {
    ///     bump.alloc(j);
    /// }
    ///```
    pub fn reset(&mut self) {
        // Takes `&mut self` so `self` must be unique and there can't be any
        // borrows active that would get invalidated by resetting.
        unsafe {
            let mut footer = Some(self.all_chunk_footers.get());

            // Reset the pointer in each of our chunks.
            while let Some(f) = footer {
                footer = f.as_ref().next.get();

                if f == self.current_chunk_footer.get() {
                    // If this is the current chunk, then reset the bump finger
                    // to the start of the chunk.
                    f.as_ref()
                        .ptr
                        .set(NonNull::new_unchecked(f.as_ref().data.as_ptr() as *mut u8));
                    f.as_ref().next.set(None);
                    self.all_chunk_footers.set(f);
                } else {
                    // If this is not the current chunk, return it to the global
                    // allocator.
                    dealloc(f.as_ref().data.as_ptr(), Bump::chunk_layout());
                }
            }

            debug_assert_eq!(
                self.all_chunk_footers.get(),
                self.current_chunk_footer.get(),
                "The current chunk should be the list head of all of our chunks"
            );
            debug_assert!(
                self.current_chunk_footer
                    .get()
                    .as_ref()
                    .next
                    .get()
                    .is_none(),
                "We should only have a single chunk"
            );
            debug_assert_eq!(
                self.current_chunk_footer.get().as_ref().ptr.get(),
                self.current_chunk_footer.get().as_ref().data,
                "Our chunk's bump finger should be reset to the start of its allocation"
            );
        }
    }

    /// Allocate an object.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let x = bump.alloc("hello");
    /// assert_eq!(*x, "hello");
    /// ```
    ///
    /// ## Panics
    ///
    /// Panics if `size_of::<T>() > 65520` or if `align_of::<T>() > 8`.
    #[inline(always)]
    pub fn alloc<T: BumpAllocSafe>(&self, val: T) -> &mut T {
        let layout = Layout::new::<T>();

        unsafe {
            let p = self.alloc_layout(layout);
            let p = p.as_ptr() as *mut T;
            ptr::write(p, val);
            &mut *p
        }
    }

    #[inline(always)]
    fn alloc_layout(&self, layout: Layout) -> NonNull<u8> {
        assert!(layout.size() <= CHUNK_SIZE);
        assert!(layout.align() <= CHUNK_ALIGN);

        unsafe {
            let footer = self.current_chunk_footer.get();
            let footer = footer.as_ref();
            let ptr = footer.ptr.get().as_ptr() as usize;
            let ptr = round_up_to(ptr, layout.align());
            let end = footer as *const _ as usize;
            debug_assert!(ptr <= end);

            if layout.size() < (end - ptr) {
                let p = ptr as *mut u8;
                let new_ptr = ptr + layout.size();
                debug_assert!(new_ptr <= footer as *const _ as usize);
                footer.ptr.set(NonNull::new_unchecked(new_ptr as *mut u8));
                return NonNull::new_unchecked(p);
            }
        }

        self.alloc_layout_slow(layout)
    }

    // Slow path allocation for when we need to allocate a new chunk from the
    // parent bump set because there isn't enough room in our current chunk.
    #[inline(never)]
    fn alloc_layout_slow(&self, layout: Layout) -> NonNull<u8> {
        debug_assert!(
            layout.size() <= CHUNK_SIZE,
            "we already check this in `alloc`"
        );
        debug_assert!(
            layout.align() <= CHUNK_ALIGN,
            "we already check this in `alloc`"
        );

        unsafe {
            // Get a new chunk from the global allocator.
            let footer = Bump::new_chunk();

            // Set our current chunk's next link to this new chunk.
            self.current_chunk_footer
                .get()
                .as_ref()
                .next
                .set(Some(footer));

            // Set the new chunk as our new current chunk.
            self.current_chunk_footer.set(footer);

            // Move the bump ptr finger ahead to allocate room for `val`.
            let footer = footer.as_ref();
            let ptr = footer.ptr.get().as_ptr() as usize + layout.size();
            debug_assert!(ptr <= footer as *const _ as usize);
            footer.ptr.set(NonNull::new_unchecked(ptr as *mut u8));

            // Return a pointer to the start of this chunk.
            footer.data.cast::<u8>()
        }
    }

    /// Call `f` on each chunk of allocated memory that this arena has bump
    /// allocated into.
    ///
    /// `f` is invoked in order of allocation: oldest chunks first, newest
    /// chunks last.
    ///
    /// ## Safety
    ///
    /// Because this method takes `&mut self`, we know that the bump arena
    /// reference is unique and therefore there aren't any active references to
    /// any of the objects we've allocated in it either. This potential aliasing
    /// of exclusive references is one common footgun for unsafe code that we
    /// don't need to worry about here.
    ///
    /// However, there could be regions of uninitilized memory used as padding
    /// between allocations. Reading uninitialized memory is big time undefined
    /// behavior!
    ///
    /// The only way to guarantee that there is no padding between allocations
    /// or within allocated objects is if all of these properties hold:
    ///
    /// 1. Every object allocated in this arena has the same alignment.
    /// 2. Every object's size is a multiple of its alignment.
    /// 3. None of the objects allocated in this arena contain any internal
    ///    padding.
    ///
    /// If you want to use this `each_allocated_chunk` method, it is *your*
    /// responsibility to ensure that these properties hold!
    ///
    /// ## Example
    ///
    /// ```
    /// let mut bump = bumpalo::Bump::new();
    ///
    /// // Allocate a bunch of things in this bump arena, potentially causing
    /// // additional memory chunks to be reserved.
    /// for i in 0..10000 {
    ///     bump.alloc(i);
    /// }
    ///
    /// // Iterate over each chunk we've bump allocated into. This is safe
    /// // because we have only allocated `usize` objects in this arena.
    /// unsafe {
    ///     bump.each_allocated_chunk(|ch| {
    ///         println!("Used a chunk that is {} bytes long", ch.len());
    ///     });
    /// }
    /// ```
    pub unsafe fn each_allocated_chunk<F>(&mut self, mut f: F)
    where
        F: for<'a> FnMut(&'a [u8]),
    {
        let mut footer = Some(self.all_chunk_footers.get());
        while let Some(foot) = footer {
            let foot = foot.as_ref();

            let start = foot.data.as_ptr() as usize;
            let end_of_allocated_region = foot.ptr.get().as_ptr() as usize;
            debug_assert!(end_of_allocated_region <= foot as *const _ as usize);
            debug_assert!(end_of_allocated_region > start);

            let len = end_of_allocated_region - start;
            debug_assert!(len <= CHUNK_SIZE);
            let slice = slice::from_raw_parts(start as *const u8, len);
            f(slice);

            footer = foot.next.get();
        }
    }
}

unsafe impl<'a> alloc::Alloc for &'a Bump {
    #[inline(always)]
    unsafe fn alloc(&mut self, layout: Layout) -> Result<NonNull<u8>, alloc::AllocErr> {
        Ok(self.alloc_layout(layout))
    }

    #[inline(always)]
    unsafe fn dealloc(&mut self, _ptr: NonNull<u8>, _layout: Layout) {}
}

#[test]
fn chunk_footer_is_three_words() {
    assert_eq!(mem::size_of::<ChunkFooter>(), mem::size_of::<usize>() * 3);
}
