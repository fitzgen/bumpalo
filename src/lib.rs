/*!

> A fast bump allocation arena for Rust.

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

See [the `BumpAllocSafe` marker trait](./trait.BumpAllocSafe.html) for details.

## Example

```
use bumpalo::{BumpSet, BumpAllocSafe};
use std::u64;

struct Doggo {
    cuteness: u64,
    age: u8,
    scritches_required: bool,
}

impl BumpAllocSafe for Doggo {}

let set = BumpSet::new();

let bump = set.new_bump();

let scooter = bump.alloc(Doggo {
    cuteness: u64::max_value(),
    age: 8,
    scritches_required: true,
});
# let _ = scooter;
```

 */

mod impls;

use std::alloc::{alloc, dealloc, Layout};
use std::cell::{Cell, UnsafeCell};
use std::mem;
use std::ptr::{self, NonNull};
use std::slice;

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
/// you can't rely on them for memory safety. Therefore, implementing this trait
/// is **not** `unsafe` in the Rust sense (which is only about memory
/// safety). But instead of taking any `T`, bump allocation requires that you
/// implement this marker trait for `T` just so that you know what you're
/// getting into.
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

/// A set of bump allocators with a shared pool of memory chunks.
pub struct BumpSet {
    free: Cell<Option<NonNull<Chunk>>>,
}

unsafe impl Sync for BumpSet {}

pub struct Bump<'a> {
    set: &'a BumpSet,
    current_chunk: Cell<NonNull<Chunk>>,
    all_chunks: Cell<NonNull<Chunk>>,
}

#[repr(C)]
struct Chunk {
    _data: [UnsafeCell<u8>; Chunk::SIZE],
    footer: ChunkFooter,
}

#[repr(C)]
struct ChunkFooter {
    next: Cell<Option<NonNull<Chunk>>>,
    ptr: Cell<NonNull<u8>>,
}

impl BumpSet {
    pub fn new() -> BumpSet {
        BumpSet {
            free: Cell::new(None),
        }
    }

    pub fn new_bump(&self) -> Bump {
        let chunk = self.chunk();
        Bump {
            set: self,
            current_chunk: Cell::new(chunk),
            all_chunks: Cell::new(chunk),
        }
    }

    fn chunk(&self) -> NonNull<Chunk> {
        unsafe {
            let chunk = alloc(Chunk::layout());
            assert!(!chunk.is_null());

            let next = Cell::new(None);
            let ptr = Cell::new(NonNull::new_unchecked(chunk));
            let footer_ptr = chunk as usize + Chunk::SIZE;
            ptr::write(footer_ptr as *mut ChunkFooter, ChunkFooter { next, ptr });

            NonNull::new_unchecked(chunk as *mut Chunk)
        }
    }
}

impl Drop for BumpSet {
    fn drop(&mut self) {
        unsafe {
            let mut chunk = self.free.get();
            while let Some(ch) = chunk {
                chunk = ch.as_ref().footer.next.get();
                dealloc(ch.as_ptr() as *mut u8, Chunk::layout());
            }
        }
    }
}

#[inline]
pub(crate) fn round_up_to(n: usize, divisor: usize) -> usize {
    debug_assert!(divisor.is_power_of_two());
    (n + divisor - 1) & !(divisor - 1)
}

impl<'a> Bump<'a> {
    /// Reset this bump allocator.
    ///
    /// Takes `&mut self` so `self` must be unique and there can't be any
    /// borrows active that would get invalidated by resetting.
    pub fn reset(&mut self) {
        unsafe {
            let mut chunk = Some(self.all_chunks.get());

            // Reset the pointer in each of our chunks.
            while let Some(ch) = chunk {
                let footer = &ch.as_ref().footer;
                footer
                    .ptr
                    .set(NonNull::new_unchecked(ch.as_ptr() as *mut u8));
                chunk = footer.next.get();

                // If this is not the current chunk, push it back to the parent set.
                if ch != self.current_chunk.get() {
                    footer.next.set(self.set.free.get());
                    self.set.free.set(Some(ch));
                }
            }

            // And reset this bump allocator's only chunk to the current chunk.
            self.current_chunk.get().as_ref().footer.next.set(None);
            self.all_chunks.set(self.current_chunk.get());
        }
    }

    /// Allocate an object.
    ///
    /// ## Panics
    ///
    /// Panics if `size_of::<T>() > 65520` or if `align_of::<T>() > 8`.
    #[inline]
    pub fn alloc<T: BumpAllocSafe>(&self, val: T) -> &mut T {
        let size = mem::size_of::<T>();
        let align = mem::align_of::<T>();
        assert!(size <= Chunk::SIZE);
        assert!(align <= Chunk::ALIGN);

        unsafe {
            let current_chunk = self.current_chunk.get();
            let footer = &current_chunk.as_ref().footer;
            let ptr = footer.ptr.get().as_ptr() as usize;
            let ptr = round_up_to(ptr, align);
            let end = footer as *const _ as usize;
            debug_assert!(ptr <= end);

            if size < (end - ptr) {
                let p = ptr as *mut T;
                ptr::write(p, val);
                let new_ptr = ptr + size;
                debug_assert!(new_ptr <= footer as *const _ as usize);
                footer.ptr.set(NonNull::new_unchecked(new_ptr as *mut u8));
                return &mut *p;
            }
        }

        self.alloc_slow(val)
    }

    // Slow path allocation for when we need to allocate a new chunk from the
    // parent bump set because there isn't enough room in our current chunk.
    #[inline(never)]
    fn alloc_slow<T: BumpAllocSafe>(&self, val: T) -> &mut T {
        let size = mem::size_of::<T>();
        let align = mem::align_of::<T>();
        debug_assert!(size <= Chunk::SIZE, "we already check this in `alloc`");
        debug_assert!(align <= Chunk::ALIGN, "we already check this in `alloc`");

        unsafe {
            // Get a new chunk from the parent bump set.
            let chunk = self.set.chunk();

            // Set our current chunk's next link to this new chunk.
            self.current_chunk
                .get()
                .as_ref()
                .footer
                .next
                .set(Some(chunk));

            // Set the new chunk as our new current chunk.
            self.current_chunk
                .set(NonNull::new_unchecked(chunk.as_ptr()));

            // Move the bump ptr finger ahead to allocate room for `val`.
            let footer = &chunk.as_ref().footer;
            let ptr = footer.ptr.get().as_ptr() as usize + size;
            debug_assert!(ptr <= footer as *const _ as usize);
            footer.ptr.set(NonNull::new_unchecked(ptr as *mut u8));

            // Write `val` into the allocated space at the start of the new
            // chunk and return a reference to it.
            let p = chunk.cast::<T>().as_ptr();
            ptr::write(p, val);
            &mut *p
        }
    }

    /// Call `f` on each chunk of allocated memory.
    ///
    /// `f` is invoked in order of allocation.
    pub fn each_allocated_chunk<F>(&mut self, mut f: F)
    where
        F: for<'b> FnMut(&'b [u8]),
    {
        // Because this method takes `&mut self` we know that there can be no
        // aliasing with references to allocated objects.
        unsafe {
            let mut chunk = Some(self.all_chunks.get());
            while let Some(ch) = chunk {
                let footer = &ch.as_ref().footer;

                let start = ch.as_ptr() as usize;
                let end_of_allocated_region = footer.ptr.get().as_ptr() as usize;
                debug_assert!(end_of_allocated_region <= footer as *const _ as usize);
                debug_assert!(end_of_allocated_region > start);

                let len = end_of_allocated_region - start;
                debug_assert!(len <= Chunk::SIZE);
                let slice = slice::from_raw_parts(start as *const u8, len);
                f(slice);

                chunk = footer.next.get();
            }
        }
    }
}

impl<'a> Drop for Bump<'a> {
    fn drop(&mut self) {
        unsafe {
            // Reset all the bump pointers in our chunks and give all but the
            // current chunk back to the parent set.
            self.reset();

            // Give the current chunk back to the parent set.
            let last_chunk = self.current_chunk.get();
            let footer = &last_chunk.as_ref().footer;
            let next = self.set.free.get();
            footer.next.set(next);
            self.set.free.set(Some(last_chunk));
        }
    }
}

// Maximum typical overhead per allocation imposed by allocators for
// wasm32-unknown-unknown.
const MALLOC_OVERHEAD: usize = 16;

impl Chunk {
    const SIZE_WITH_FOOTER: usize = (1 << 16) - MALLOC_OVERHEAD;
    const SIZE: usize = Chunk::SIZE_WITH_FOOTER - mem::size_of::<ChunkFooter>();
    const ALIGN: usize = 8;

    #[inline]
    fn layout() -> Layout {
        Layout::from_size_align(Chunk::SIZE_WITH_FOOTER, Chunk::ALIGN).unwrap()
    }
}

#[test]
fn chunk_footer_is_two_words() {
    assert_eq!(mem::size_of::<ChunkFooter>(), mem::size_of::<usize>() * 2);
}
