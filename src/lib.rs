/*!

**A fast bump allocation arena for Rust.**

[![](https://docs.rs/bumpalo/badge.svg)](https://docs.rs/bumpalo/)
[![](https://img.shields.io/crates/v/bumpalo.svg)](https://crates.io/crates/bumpalo)
[![](https://img.shields.io/crates/d/bumpalo.svg)](https://crates.io/crates/bumpalo)
[![Build Status](https://dev.azure.com/fitzgen/bumpalo/_apis/build/status/fitzgen.bumpalo?branchName=master)](https://dev.azure.com/fitzgen/bumpalo/_build/latest?definitionId=2&branchName=master)

![](https://github.com/fitzgen/bumpalo/raw/master/bumpalo.png)

## Bump Allocation

Bump allocation is a fast, but limited approach to allocation. We have a chunk
of memory, and we maintain a pointer within that memory. Whenever we allocate an
object, we do a quick test that we have enough capacity left in our chunk to
allocate the object and then update the pointer by the object's size. *That's
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

## What happens when the memory chunk is full?

This implementation will allocate a new memory chunk from the global allocator
and then start bump allocating into this new memory chunk.

## Example

```
use bumpalo::Bump;
use std::u64;

struct Doggo {
    cuteness: u64,
    age: u8,
    scritches_required: bool,
}

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

## Collections

When the on-by-default `"collections"` feature is enabled, a fork of some of the
`std` library's collections are available in the `collections` module. These
collection types are modified to allocate their space inside `bumpalo::Bump`
arenas.

```rust
# #[cfg(feature = "collections")]
# {
use bumpalo::{Bump, collections::Vec};

// Create a new bump arena.
let bump = Bump::new();

// Create a vector of integers whose storage is backed by the bump arena. The
// vector cannot outlive its backing arena, and this property is enforced with
// Rust's lifetime rules.
let mut v = Vec::new_in(&bump);

// Push a bunch of integers onto `v`!
for i in 0..100 {
    v.push(i);
}
# }
```

Eventually [all `std` collection types will be parameterized by an
allocator](https://github.com/rust-lang/rust/issues/42774) and we can remove
this `collections` module and use the `std` versions.

## `#![no_std]` Support

Bumpalo is a `no_std` crate. It depends only on the `alloc` and `core` crates.

 */

#![deny(missing_debug_implementations)]
#![deny(missing_docs)]
#![no_std]

extern crate alloc as core_alloc;

#[cfg(feature = "collections")]
pub mod collections;

mod alloc;

use core::cell::Cell;
use core::cmp;
use core::iter;
use core::marker::PhantomData;
use core::mem;
use core::ptr::{self, NonNull};
use core::slice;
use core_alloc::alloc::{alloc, dealloc, Layout};

/// An arena to bump allocate into.
///
/// ## No `Drop`s
///
/// Objects that are bump-allocated will never have their `Drop` implementation
/// called &mdash; unless you do it manually yourself. This makes it relatively
/// easy to leak memory or other resources.
///
/// If you have a type which internally manages
///
/// * an allocation from the global heap (e.g. `Vec<T>`),
/// * open file descriptors (e.g. `std::fs::File`), or
/// * any other resource that must be cleaned up (e.g. an `mmap`)
///
/// and relies on its `Drop` implementation to clean up the internal resource,
/// then if you allocate that type with a `Bump`, you need to find a new way to
/// clean up after it yourself.
///
/// Potential solutions are
///
/// * calling [`drop_in_place`][drop_in_place] or using
///   [`std::mem::ManuallyDrop`][manuallydrop] to manually drop these types,
/// * using `bumpalo::collections::Vec` instead of `std::vec::Vec`, or
/// * simply avoiding allocating these problematic types within a `Bump`.
///
/// Note that not calling `Drop` is memory safe! Destructors are never
/// guaranteed to run in Rust, you can't rely on them for enforcing memory
/// safety.
///
/// [drop_in_place]: https://doc.rust-lang.org/stable/std/ptr/fn.drop_in_place.html
/// [manuallydrop]: https://doc.rust-lang.org/stable/std/mem/struct.ManuallyDrop.html
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
}

#[repr(C)]
#[derive(Debug)]
struct ChunkFooter {
    // Pointer to the start of this chunk allocation. This footer is always at
    // the end of the chunk.
    data: NonNull<u8>,

    // The layout of this chunk's allocation.
    layout: Layout,

    // Link to the previous chunk, if any.
    prev: Cell<Option<NonNull<ChunkFooter>>>,

    // Bump allocation finger that is always in the range `self.data..=self`.
    ptr: Cell<NonNull<u8>>,

    // Pointer to the end of the first allocation made in this chunk.
    // Used in iter_allocated_chunks to avoid giving back padding bytes
    // that are outside the user's control
    end_of_first_allocation: Cell<Option<NonNull<u8>>>,
}

impl Default for Bump {
    fn default() -> Bump {
        Bump::new()
    }
}

impl Drop for Bump {
    fn drop(&mut self) {
        unsafe {
            dealloc_chunk_list(Some(self.current_chunk_footer.get()));
        }
    }
}

#[inline]
unsafe fn dealloc_chunk_list(mut footer: Option<NonNull<ChunkFooter>>) {
    while let Some(f) = footer {
        footer = f.as_ref().prev.get();
        dealloc(f.as_ref().data.as_ptr(), f.as_ref().layout);
    }
}

// `Bump`s are safe to send between threads because nothing aliases its owned
// chunks until you start allocating from it. But by the time you allocate from
// it, the returned references to allocations borrow the `Bump` and therefore
// prevent sending the `Bump` across threads until the borrows end.
unsafe impl Send for Bump {}

#[inline]
pub(crate) fn round_up_to(n: usize, divisor: usize) -> Option<usize> {
    debug_assert!(divisor > 0);
    debug_assert!(divisor.is_power_of_two());
    Some(n.checked_add(divisor - 1)? & !(divisor - 1))
}

// Maximum typical overhead per allocation imposed by allocators.
const MALLOC_OVERHEAD: usize = 16;

// Choose a relatively small default initial chunk size, since we double chunk
// sizes as we grow bump arenas to amortize costs of hitting the global
// allocator.
const DEFAULT_CHUNK_SIZE_WITH_FOOTER: usize = (1 << 9) - MALLOC_OVERHEAD;
const DEFAULT_CHUNK_ALIGN: usize = mem::align_of::<ChunkFooter>();

/// Wrapper around `Layout::from_size_align` that adds debug assertions.
#[inline]
unsafe fn layout_from_size_align(size: usize, align: usize) -> Layout {
    if cfg!(debug_assertions) {
        Layout::from_size_align(size, align).unwrap()
    } else {
        Layout::from_size_align_unchecked(size, align)
    }
}

#[inline(never)]
fn allocation_size_overflow<T>() -> T {
    panic!("requested allocation size overflowed")
}

impl Bump {
    fn default_chunk_layout() -> Layout {
        unsafe { layout_from_size_align(DEFAULT_CHUNK_SIZE_WITH_FOOTER, DEFAULT_CHUNK_ALIGN) }
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
        let chunk_footer = Self::new_chunk(None, None);
        Bump {
            current_chunk_footer: Cell::new(chunk_footer),
        }
    }

    /// Construct a new arena with the specified capacity to bump allocate into.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::with_capacity(100);
    /// # let _ = bump;
    /// ```
    pub fn with_capacity(capacity: usize) -> Bump {
        let chunk_footer = Self::new_chunk(
            Some((DEFAULT_CHUNK_SIZE_WITH_FOOTER, unsafe {
                layout_from_size_align(capacity, 1)
            })),
            None,
        );
        Bump {
            current_chunk_footer: Cell::new(chunk_footer),
        }
    }

    /// Allocate a new chunk and return its initialized footer.
    ///
    /// If given, `layouts` is a tuple of the current chunk size and the
    /// layout of the allocation request that triggered us to fall back to
    /// allocating a new chunk of memory.
    fn new_chunk(
        layouts: Option<(usize, Layout)>,
        prev: Option<NonNull<ChunkFooter>>,
    ) -> NonNull<ChunkFooter> {
        unsafe {
            let layout: Layout =
                layouts.map_or_else(Bump::default_chunk_layout, |(old_size, requested)| {
                    let old_doubled = old_size.checked_mul(2).unwrap();
                    let footer_align = mem::align_of::<ChunkFooter>();
                    debug_assert_eq!(
                        old_doubled,
                        round_up_to(old_doubled, footer_align).unwrap(),
                        "The old size was already a multiple of our chunk footer alignment, so no \
                         need to round it up again."
                    );

                    // Have a reasonable "doubling behavior" but ensure that if
                    // a very large size is requested we round up to that.
                    let size_to_allocate = cmp::max(old_doubled, requested.size());

                    // Handle size/alignment of our allocated chunk, taking into
                    // account an overaligned allocation if one is required.
                    // Note that we also add to the size a `ChunkFooter` because
                    // we'll be placing one at the end, and we need to at least
                    // satisfy `requested.size()` bytes.
                    let size = cmp::max(
                        size_to_allocate,
                        requested.size() + mem::size_of::<ChunkFooter>(),
                    );
                    let size =
                        round_up_to(size, footer_align).unwrap_or_else(allocation_size_overflow);
                    let align = cmp::max(footer_align, requested.align());

                    layout_from_size_align(size, align)
                });

            let size = layout.size();
            debug_assert_eq!(layout.align() % mem::align_of::<ChunkFooter>(), 0);

            let data = alloc(layout);
            let data = NonNull::new(data).unwrap_or_else(|| oom());

            // The `ChunkFooter` is at the end of the chunk.
            let footer_ptr = data.as_ptr() as usize + size - mem::size_of::<ChunkFooter>();
            debug_assert_eq!(footer_ptr % mem::align_of::<ChunkFooter>(), 0);
            let footer_ptr = footer_ptr as *mut ChunkFooter;

            // The bump pointer is initialized to the end of the range we will
            // bump out of.
            let ptr = Cell::new(NonNull::new_unchecked(footer_ptr as *mut u8));

            ptr::write(
                footer_ptr,
                ChunkFooter {
                    data,
                    layout,
                    prev: Cell::new(prev),
                    ptr,
                    end_of_first_allocation: Cell::new(None),
                },
            );

            NonNull::new_unchecked(footer_ptr)
        }
    }

    /// Reset this bump allocator.
    ///
    /// Performs mass deallocation on everything allocated in this arena by
    /// resetting the pointer into the underlying chunk of memory to the start
    /// of the chunk. Does not run any `Drop` implementations on deallocated
    /// objects; see [the `Bump` type's top-level
    /// documentation](./struct.Bump.html) for details.
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
            let cur_chunk = self.current_chunk_footer.get();

            // Deallocate all chunks except the current one
            let prev_chunk = cur_chunk.as_ref().prev.replace(None);
            dealloc_chunk_list(prev_chunk);

            // Reset the bump finger to the end of the chunk.
            cur_chunk.as_ref().ptr.set(cur_chunk.cast());
            cur_chunk.as_ref().end_of_first_allocation.set(None);

            debug_assert!(
                self.current_chunk_footer
                    .get()
                    .as_ref()
                    .prev
                    .get()
                    .is_none(),
                "We should only have a single chunk"
            );
            debug_assert_eq!(
                self.current_chunk_footer.get().as_ref().ptr.get(),
                self.current_chunk_footer.get().cast(),
                "Our chunk's bump finger should be reset to the start of its allocation"
            );
        }
    }

    /// Allocate an object in this `Bump` and return an exclusive reference to
    /// it.
    ///
    /// ## Panics
    ///
    /// Panics if reserving space for `T` would cause an overflow.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let x = bump.alloc("hello");
    /// assert_eq!(*x, "hello");
    /// ```
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    pub fn alloc<T>(&self, val: T) -> &mut T {
        self.alloc_with(|| val)
    }

    /// Pre-allocate space for an object in this `Bump`, initializes it using
    /// the closure, then returns an exclusive reference to it.
    ///
    /// Calling `bump.alloc(x)` is essentially equivalent to calling
    /// `bump.alloc_with(|| x)`. However if you use `alloc_with`, then the
    /// closure will not be invoked until after allocating space for storing
    /// `x` on the heap.
    ///
    /// This can be useful in certain edge-cases related to compiler
    /// optimizations. When evaluating `bump.alloc(x)`, semantically `x` is
    /// first put on the stack and then moved onto the heap. In some cases,
    /// the compiler is able to optimize this into constructing `x` directly
    /// on the heap, however in many cases it does not.
    ///
    /// The function `alloc_with` tries to help the compiler be smarter. In
    /// most cases doing `bump.alloc_with(|| x)` on release mode will be
    /// enough to help the compiler to realize this optimization is valid
    /// and construct `x` directly onto the heap.
    ///
    /// ## Warning
    ///
    /// This function critically depends on compiler optimizations to achieve
    /// its desired effect. This means that it is not an effective tool when
    /// compiling without optimizations on.
    ///
    /// Even when optimizations are on, this function does not **guarantee**
    /// that the value is constructed on the heap. To the best of our
    /// knowledge no such guarantee can be made in stable Rust as of 1.33.
    ///
    /// ## Panics
    ///
    /// Panics if reserving space for `T` would cause an overflow.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let x = bump.alloc_with(|| "hello");
    /// assert_eq!(*x, "hello");
    /// ```
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    pub fn alloc_with<F, T>(&self, f: F) -> &mut T
    where
        F: FnOnce() -> T,
    {
        #[inline(always)]
        unsafe fn inner_writer<T, F>(ptr: *mut T, f: F)
        where
            F: FnOnce() -> T,
        {
            // This function is translated as:
            // - allocate space for a T on the stack
            // - call f() with the return value being put onto this stack space
            // - memcpy from the stack to the heap
            //
            // Ideally we want LLVM to always realize that doing a stack
            // allocation is unnecessary and optimize the code so it writes
            // directly into the heap instead. It seems we get it to realize
            // this most consistently if we put this critical line into it's
            // own function instead of inlining it into the surrounding code.
            ptr::write(ptr, f())
        }

        let layout = Layout::new::<T>();

        unsafe {
            let p = self.alloc_layout(layout);
            let p = p.as_ptr() as *mut T;
            inner_writer(p, f);
            &mut *p
        }
    }

    /// `Copy` a slice into this `Bump` and return an exclusive reference to
    /// the copy.
    ///
    /// ## Panics
    ///
    /// Panics if reserving space for the slice would cause an overflow.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let x = bump.alloc_slice_copy(&[1, 2, 3]);
    /// assert_eq!(x, &[1, 2, 3]);
    /// ```
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    pub fn alloc_slice_copy<T>(&self, src: &[T]) -> &mut [T]
    where
        T: Copy,
    {
        let layout = Layout::for_value(src);
        let dst = self.alloc_layout(layout).cast::<T>();

        unsafe {
            ptr::copy_nonoverlapping(src.as_ptr(), dst.as_ptr(), src.len());
            slice::from_raw_parts_mut(dst.as_ptr(), src.len())
        }
    }

    /// `Clone` a slice into this `Bump` and return an exclusive reference to
    /// the clone. Prefer `alloc_slice_copy` if `T` is `Copy`.
    ///
    /// ## Panics
    ///
    /// Panics if reserving space for the slice would cause an overflow.
    ///
    /// ## Example
    ///
    /// ```
    /// #[derive(Clone, Debug, Eq, PartialEq)]
    /// struct Sheep {
    ///     name: String,
    /// }
    ///
    /// let originals = vec![
    ///     Sheep { name: "Alice".into() },
    ///     Sheep { name: "Bob".into() },
    ///     Sheep { name: "Cathy".into() },
    /// ];
    ///
    /// let bump = bumpalo::Bump::new();
    /// let clones = bump.alloc_slice_clone(&originals);
    /// assert_eq!(originals, clones);
    /// ```
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    pub fn alloc_slice_clone<T>(&self, src: &[T]) -> &mut [T]
    where
        T: Clone,
    {
        let layout = Layout::for_value(src);
        let dst = self.alloc_layout(layout).cast::<T>();

        unsafe {
            for (i, val) in src.iter().cloned().enumerate() {
                ptr::write(dst.as_ptr().add(i), val);
            }

            slice::from_raw_parts_mut(dst.as_ptr(), src.len())
        }
    }

    /// Allocate space for an object with the given `Layout`.
    ///
    /// The returned pointer points at uninitialized memory, and should be
    /// initialized with
    /// [`std::ptr::write`](https://doc.rust-lang.org/stable/std/ptr/fn.write.html).
    #[inline(always)]
    pub fn alloc_layout(&self, layout: Layout) -> NonNull<u8> {
        if let Some(p) = self.try_alloc_layout_fast(layout) {
            p
        } else {
            self.alloc_layout_slow(layout)
        }
    }

    #[inline(always)]
    fn try_alloc_layout_fast(&self, layout: Layout) -> Option<NonNull<u8>> {
        unsafe {
            let footer = self.current_chunk_footer.get();
            let footer = footer.as_ref();
            let initial_ptr = footer.ptr.get().as_ptr() as usize;
            let start = footer.data.as_ptr() as usize;
            debug_assert!(start <= initial_ptr);
            debug_assert!(initial_ptr <= footer as *const _ as usize);

            let ptr = initial_ptr.checked_sub(layout.size())?;
            let aligned_ptr = ptr & !(layout.align() - 1);

            if aligned_ptr >= start {
                // We might be the very first allocation made in the entire bump,
                // in which case we need to set end_of_first_allocation
                // for the first chunk
                if initial_ptr == footer as *const ChunkFooter as usize {
                    footer
                        .end_of_first_allocation
                        .set(Some(NonNull::new_unchecked(
                            (aligned_ptr + layout.size()) as *mut u8,
                        )));
                }
                let aligned_ptr = NonNull::new_unchecked(aligned_ptr as *mut u8);
                footer.ptr.set(aligned_ptr);
                Some(aligned_ptr)
            } else {
                None
            }
        }
    }

    // Slow path allocation for when we need to allocate a new chunk from the
    // parent bump set because there isn't enough room in our current chunk.
    #[inline(never)]
    fn alloc_layout_slow(&self, layout: Layout) -> NonNull<u8> {
        unsafe {
            let size = layout.size();

            // Get a new chunk from the global allocator.
            let current_footer = self.current_chunk_footer.get();
            let current_layout = current_footer.as_ref().layout;
            let new_footer =
                Bump::new_chunk(Some((current_layout.size(), layout)), Some(current_footer));
            debug_assert_eq!(
                new_footer.as_ref().data.as_ptr() as usize % layout.align(),
                0
            );

            // Set the new chunk as our new current chunk.
            self.current_chunk_footer.set(new_footer);

            let new_footer = new_footer.as_ref();

            // Move the bump ptr finger down to allocate room for `val`. We know
            // this can't overflow because we successfully allocated a chunk of
            // at least the requested size.
            let ptr = new_footer.ptr.get().as_ptr() as usize - size;
            // Round the pointer down to the requested alignment.
            let ptr = ptr & !(layout.align() - 1);
            debug_assert!(
                ptr <= new_footer as *const _ as usize,
                "{:#x} <= {:#x}",
                ptr,
                new_footer as *const _ as usize
            );

            new_footer
                .end_of_first_allocation
                .set(Some(NonNull::new_unchecked(
                    (ptr + layout.size()) as *mut u8,
                )));

            let ptr = NonNull::new_unchecked(ptr as *mut u8);
            new_footer.ptr.set(ptr);

            // Return a pointer to the freshly allocated region in this chunk.
            ptr
        }
    }

    /// Returns an iterator over each chunk of allocated memory that
    /// this arena has bump allocated into.
    ///
    /// The chunks are returned ordered by allocation time, with the most recently
    /// allocated chunk being returned first.
    ///
    /// The values inside each chunk is also ordered by allocation time, with the most
    /// recent allocation being earlier in the slice.
    ///
    /// ## Safety
    ///
    /// Because this method takes `&mut self`, we know that the bump arena
    /// reference is unique and therefore there aren't any active references to
    /// any of the objects we've allocated in it either. This potential aliasing
    /// of exclusive references is one common footgun for unsafe code that we
    /// don't need to worry about here.
    ///
    /// However, there could be regions of uninitialized memory used as padding
    /// between allocations, which is why this iterator has items of type
    /// `[MaybeUninit<u8>]`, instead of simply `[u8]`.
    ///
    /// The only way to guarantee that there is no padding between allocations
    /// or within allocated objects is if all of these properties hold:
    ///
    /// 1. Every object allocated in this arena has the same alignment.
    /// 2. Every object's size is a multiple of its alignment.
    /// 3. None of the objects allocated in this arena contain any internal
    ///    padding.
    ///
    /// If you want to use this `iter_allocated_chunks` method, it is *your*
    /// responsibility to ensure that these properties hold before calling
    /// `MaybeUninit::assume_init` or otherwise reading the returned values.
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
    /// // because we have only allocated `i32` objects in this arena.
    /// for ch in bump.iter_allocated_chunks() {
    ///     println!("Used a chunk that is {} bytes long", ch.len());
    ///     println!("The first byte is {:?}", unsafe { ch.get(0).unwrap().assume_init() });
    /// }
    /// ```
    pub fn iter_allocated_chunks(&mut self) -> ChunkIter<'_> {
        ChunkIter {
            footer: Some(self.current_chunk_footer.get()),
            bump: PhantomData,
        }
    }

    /// Call `f` on each chunk of allocated memory that this arena has bump
    /// allocated into.
    ///
    /// `f` is invoked in order of allocation: newest chunks first, oldest
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
    /// However, there could be regions of uninitialized memory used as padding
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
    /// // because we have only allocated `i32` objects in this arena.
    /// unsafe {
    ///     bump.each_allocated_chunk(|ch| {
    ///         println!("Used a chunk that is {} bytes long", ch.len());
    ///     });
    /// }
    /// ```
    #[deprecated(note = "deprecated in favor of iter_allocated_chunks")]
    pub unsafe fn each_allocated_chunk<F>(&mut self, mut f: F)
    where
        F: for<'a> FnMut(&'a [u8]),
    {
        for chunk in self.iter_allocated_chunks() {
            f(slice::from_raw_parts(
                chunk.as_ptr() as *const u8,
                chunk.len(),
            ));
        }
    }

    #[inline]
    unsafe fn is_last_allocation(&self, ptr: NonNull<u8>) -> bool {
        let footer = self.current_chunk_footer.get();
        let footer = footer.as_ref();
        footer.ptr.get() == ptr
    }
}

/// An iterator over each chunk of allocated memory that
/// an arena has bump allocated into.
///
/// The chunks are returned ordered by allocation time, with the most recently
/// allocated chunk being returned first.
///
/// The values inside each chunk is also ordered by allocation time, with the most
/// recent allocation being earlier in the slice.
///
/// This struct is created by the [`iter_allocated_chunks`] method on
/// [`Bump`]. See that function for a safety description regarding reading from the returned items.
///
/// [`Bump`]: ./struct.Bump.html
/// [`iter_allocated_chunks`]: ./struct.Bump.html#method.iter_allocated_chunks
#[derive(Debug)]
pub struct ChunkIter<'a> {
    footer: Option<NonNull<ChunkFooter>>,
    bump: PhantomData<&'a mut Bump>,
}

impl<'a> Iterator for ChunkIter<'a> {
    type Item = &'a [mem::MaybeUninit<u8>];
    fn next(&mut self) -> Option<&'a [mem::MaybeUninit<u8>]> {
        unsafe {
            let foot = self.footer?;
            let foot = foot.as_ref();
            self.footer = foot.prev.get();

            let data = foot.data.as_ptr() as usize;
            let ptr = foot.ptr.get().as_ptr() as usize;

            debug_assert!(data <= ptr);
            debug_assert!(ptr <= foot as *const _ as usize);

            // Have we allocated in this chunk?
            if let Some(end_of_first_allocation) = foot.end_of_first_allocation.get() {
                let end_of_first_allocation = end_of_first_allocation.as_ptr() as usize;
                debug_assert!(ptr <= end_of_first_allocation);
                let len = end_of_first_allocation - ptr;
                let slice = slice::from_raw_parts(ptr as *const mem::MaybeUninit<u8>, len);
                Some(slice)
            } else {
                // If we have not allocated, then we must be the very first chunk
                debug_assert!(
                    foot.prev.get().is_none(),
                    "Empty chunk, but chunk has a prev pointer"
                );
                None
            }
        }
    }
}

impl<'a> iter::FusedIterator for ChunkIter<'a> {}

#[inline(never)]
#[cold]
fn oom() -> ! {
    panic!("out of memory")
}

unsafe impl<'a> alloc::Alloc for &'a Bump {
    #[inline(always)]
    unsafe fn alloc(&mut self, layout: Layout) -> Result<NonNull<u8>, alloc::AllocErr> {
        Ok(self.alloc_layout(layout))
    }

    #[inline]
    unsafe fn dealloc(&mut self, ptr: NonNull<u8>, layout: Layout) {
        // If the pointer is the last allocation we made, we can reuse the bytes,
        // otherwise they are simply leaked -- at least until somebody calls reset().
        if self.is_last_allocation(ptr) {
            let ptr = NonNull::new_unchecked(ptr.as_ptr().add(layout.size()));
            self.current_chunk_footer.get().as_ref().ptr.set(ptr);
            // We could try to detect if the chunk is now empty by
            // comparing ptr to end_of_first_allocation, however this would
            // only save a few padding bytes in a few rare cases. It would
            // also mean that we would need to handle empty chunks
            // in iter_allocated_chunks, so it is probably not worth it.
            // Instead we just accept that those bytes are gone.
        }
    }

    #[inline]
    unsafe fn realloc(
        &mut self,
        ptr: NonNull<u8>,
        layout: Layout,
        new_size: usize,
    ) -> Result<NonNull<u8>, alloc::AllocErr> {
        let old_size = layout.size();

        if new_size <= old_size {
            if self.is_last_allocation(ptr)
                // Only reclaim the excess space (which requires a copy) if it
                // is worth it: we are actually going to recover "enough" space
                // and we can do a non-overlapping copy.
                && new_size <= old_size / 2
            {
                let delta = old_size - new_size;
                let footer = self.current_chunk_footer.get();
                let footer = footer.as_ref();
                footer
                    .ptr
                    .set(NonNull::new_unchecked(footer.ptr.get().as_ptr().add(delta)));
                let new_ptr = footer.ptr.get();
                // NB: we know it is non-overlapping because of the size check
                // in the `if` condition.
                ptr::copy_nonoverlapping(ptr.as_ptr(), new_ptr.as_ptr(), new_size);
                return Ok(new_ptr);
            } else {
                return Ok(ptr);
            }
        }

        if self.is_last_allocation(ptr) {
            // Try to allocate the delta size within this same block so we can
            // reuse the currently allocated space.
            let delta = new_size - old_size;
            if let Some(p) =
                self.try_alloc_layout_fast(layout_from_size_align(delta, layout.align()))
            {
                ptr::copy(ptr.as_ptr(), p.as_ptr(), new_size);
                return Ok(p);
            }
        }

        // Fallback: do a fresh allocation and copy the existing data into it.
        let new_layout = layout_from_size_align(new_size, layout.align());
        let new_ptr = self.alloc_layout(new_layout);
        ptr::copy_nonoverlapping(ptr.as_ptr(), new_ptr.as_ptr(), old_size);
        Ok(new_ptr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_footer_is_six_words() {
        assert_eq!(mem::size_of::<ChunkFooter>(), mem::size_of::<usize>() * 6);
    }

    #[test]
    #[allow(clippy::cognitive_complexity)]
    fn test_realloc() {
        use crate::alloc::Alloc;

        unsafe {
            const CAPACITY: usize = 1000;
            let mut b = Bump::with_capacity(CAPACITY);

            // `realloc` doesn't shrink allocations that aren't "worth it".
            let layout = Layout::from_size_align(100, 1).unwrap();
            let p = b.alloc_layout(layout);
            let q = (&b).realloc(p, layout, 51).unwrap();
            assert_eq!(p, q);
            b.reset();

            // `realloc` will shrink allocations that are "worth it".
            let layout = Layout::from_size_align(100, 1).unwrap();
            let p = b.alloc_layout(layout);
            let q = (&b).realloc(p, layout, 50).unwrap();
            assert!(p != q);
            b.reset();

            // `realloc` will reuse the last allocation when growing.
            let layout = Layout::from_size_align(10, 1).unwrap();
            let p = b.alloc_layout(layout);
            let q = (&b).realloc(p, layout, 11).unwrap();
            assert_eq!(q.as_ptr() as usize, p.as_ptr() as usize - 1);
            b.reset();

            // `realloc` will allocate a new chunk when growing the last
            // allocation, if need be.
            let layout = Layout::from_size_align(1, 1).unwrap();
            let p = b.alloc_layout(layout);
            let q = (&b).realloc(p, layout, CAPACITY + 1).unwrap();
            assert!(q.as_ptr() as usize != p.as_ptr() as usize - CAPACITY);
            b = Bump::with_capacity(CAPACITY);

            // `realloc` will allocate and copy when reallocating anything that
            // wasn't the last allocation.
            let layout = Layout::from_size_align(1, 1).unwrap();
            let p = b.alloc_layout(layout);
            let _ = b.alloc_layout(layout);
            let q = (&b).realloc(p, layout, 2).unwrap();
            assert!(q.as_ptr() as usize != p.as_ptr() as usize - 1);
            b.reset();
        }
    }
}
