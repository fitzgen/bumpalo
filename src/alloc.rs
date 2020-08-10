// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(unstable_name_collisions)]
#![allow(dead_code)]
#![allow(unused_unsafe)]

//! Memory allocation APIs

use core::mem;
use core::usize;

pub use core::alloc::{Layout, LayoutErr};

fn new_layout_err() -> LayoutErr {
    Layout::from_size_align(1, 3).unwrap_err()
}

pub trait UnstableLayoutMethods {
    fn padding_needed_for(&self, align: usize) -> usize;
    fn repeat(&self, n: usize) -> Result<(Layout, usize), LayoutErr>;
    fn array<T>(n: usize) -> Result<Layout, LayoutErr>;
}

impl UnstableLayoutMethods for Layout {
    fn padding_needed_for(&self, align: usize) -> usize {
        let len = self.size();

        // Rounded up value is:
        //   len_rounded_up = (len + align - 1) & !(align - 1);
        // and then we return the padding difference: `len_rounded_up - len`.
        //
        // We use modular arithmetic throughout:
        //
        // 1. align is guaranteed to be > 0, so align - 1 is always
        //    valid.
        //
        // 2. `len + align - 1` can overflow by at most `align - 1`,
        //    so the &-mask wth `!(align - 1)` will ensure that in the
        //    case of overflow, `len_rounded_up` will itself be 0.
        //    Thus the returned padding, when added to `len`, yields 0,
        //    which trivially satisfies the alignment `align`.
        //
        // (Of course, attempts to allocate blocks of memory whose
        // size and padding overflow in the above manner should cause
        // the allocator to yield an error anyway.)

        let len_rounded_up = len.wrapping_add(align).wrapping_sub(1) & !align.wrapping_sub(1);
        len_rounded_up.wrapping_sub(len)
    }

    fn repeat(&self, n: usize) -> Result<(Layout, usize), LayoutErr> {
        let padded_size = self
            .size()
            .checked_add(self.padding_needed_for(self.align()))
            .ok_or_else(new_layout_err)?;
        let alloc_size = padded_size.checked_mul(n).ok_or_else(new_layout_err)?;

        unsafe {
            // self.align is already known to be valid and alloc_size has been
            // padded already.
            Ok((
                Layout::from_size_align_unchecked(alloc_size, self.align()),
                padded_size,
            ))
        }
    }

    fn array<T>(n: usize) -> Result<Layout, LayoutErr> {
        Layout::new::<T>().repeat(n).map(|(k, offs)| {
            debug_assert!(offs == mem::size_of::<T>());
            k
        })
    }
}

#[cfg(feature = "nightly")]
pub use core_alloc::alloc::{handle_alloc_error, AllocErr, AllocRef, MemoryBlock, ReallocPlacement, AllocInit};

#[cfg(not(feature = "nightly"))]
pub use shim::*;

#[cfg(not(feature = "nightly"))]
mod shim {

    use core::fmt;
    use core::ptr::{self, NonNull};
    use core::usize;

    pub use core::alloc::{Layout, LayoutErr};

    pub fn handle_alloc_error(layout: Layout) -> ! {
        panic!("encountered allocation error: {:?}", layout)
    }

    /// The `AllocErr` error indicates an allocation failure
    /// that may be due to resource exhaustion or to
    /// something wrong when combining the given input arguments with this
    /// allocator.
    //#[unstable(feature = "allocator_api", issue = "32838")]
    #[derive(Copy, Clone, PartialEq, Eq, Debug)]
    pub struct AllocErr;

    // (we need this for downstream impl of trait Error)
    //#[unstable(feature = "allocator_api", issue = "32838")]
    impl fmt::Display for AllocErr {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("memory allocation failed")
        }
    }

    /// A desired initial state for allocated memory.
    #[derive(Debug, Copy, Clone, PartialEq, Eq)]
    //#[unstable(feature = "allocator_api", issue = "32838")]
    pub enum AllocInit {
        /// The contents of the new memory are uninitialized.
        Uninitialized,
        /// The new memory is guaranteed to be zeroed.
        Zeroed,
    }

    impl AllocInit {
        /// Initialize the specified memory block.
        ///
        /// This behaves like calling [`AllocInit::init_offset(memory, 0)`][off].
        ///
        /// [off]: AllocInit::init_offset
        ///
        /// # Safety
        ///
        /// * `memory.ptr` must be [valid] for writes of `memory.size` bytes.
        ///
        /// [valid]: ../../core/ptr/index.html#safety
        #[inline]
        //#[unstable(feature = "allocator_api", issue = "32838")]
        pub unsafe fn init(self, memory: MemoryBlock) {
            // SAFETY: the safety contract for `init_offset` must be
            // upheld by the caller.
            unsafe { self.init_offset(memory, 0) }
        }

        /// Initialize the memory block like specified by `init` at the specified `offset`.
        ///
        /// This is a no-op for [`AllocInit::Uninitialized`][] and writes zeroes for
        /// [`AllocInit::Zeroed`][] at `ptr + offset` until `ptr + layout.size()`.
        ///
        /// # Safety
        ///
        /// * `memory.ptr` must be [valid] for writes of `memory.size` bytes.
        /// * `offset` must be smaller than or equal to `memory.size`
        ///
        /// [valid]: ../../core/ptr/index.html#safety
        #[inline]
        //#[unstable(feature = "allocator_api", issue = "32838")]
        pub unsafe fn init_offset(self, memory: MemoryBlock, offset: usize) {
            debug_assert!(
                offset <= memory.size,
                "`offset` must be smaller than or equal to `memory.size`"
            );
            match self {
                AllocInit::Uninitialized => (),
                AllocInit::Zeroed => {
                    // SAFETY: the caller must guarantee that `offset` is smaller than or equal to `memory.size`,
                    // so the memory from `memory.ptr + offset` of length `memory.size - offset`
                    // is guaranteed to be contaned in `memory` and thus valid for writes.
                    unsafe { memory.ptr.as_ptr().add(offset).write_bytes(0, memory.size - offset) }
                }
            }
        }
    }

    /// Represents a block of allocated memory returned by an allocator.
    #[derive(Debug, Copy, Clone)]
    //#[unstable(feature = "allocator_api", issue = "32838")]
    pub struct MemoryBlock {
        pub ptr: NonNull<u8>,
        pub size: usize,
    }

    /// A placement constraint when growing or shrinking an existing allocation.
    #[derive(Debug, Copy, Clone, PartialEq, Eq)]
    //#[unstable(feature = "allocator_api", issue = "32838")]
    pub enum ReallocPlacement {
        /// The allocator is allowed to move the allocation to a different memory address.
        // FIXME(wg-allocators#46): Add a section to the module documentation "What is a legal
        //                          allocator" and link it at "valid location".
        ///
        /// If the allocation _does_ move, it's the responsibility of the allocator
        /// to also move the data from the previous location to the new location.
        MayMove,
        /// The address of the new memory must not change.
        ///
        /// If the allocation would have to be moved to a new location to fit, the
        /// reallocation request will fail.
        InPlace,
    }

    /// An implementation of `AllocRef` can allocate, grow, shrink, and deallocate arbitrary blocks of
    /// data described via [`Layout`][].
    ///
    /// `AllocRef` is designed to be implemented on ZSTs, references, or smart pointers because having
    /// an allocator like `MyAlloc([u8; N])` cannot be moved, without updating the pointers to the
    /// allocated memory.
    ///
    /// Unlike [`GlobalAlloc`][], zero-sized allocations are allowed in `AllocRef`. If an underlying
    /// allocator does not support this (like jemalloc) or return a null pointer (such as
    /// `libc::malloc`), this must be caught by the implementation.
    ///
    /// ### Currently allocated memory
    ///
    /// Some of the methods require that a memory block be *currently allocated* via an allocator. This
    /// means that:
    ///
    /// * the starting address for that memory block was previously returned by [`alloc`], [`grow`], or
    ///   [`shrink`], and
    ///
    /// * the memory block has not been subsequently deallocated, where blocks are either deallocated
    ///   directly by being passed to [`dealloc`] or were changed by being passed to [`grow`] or
    ///   [`shrink`] that returns `Ok`. If `grow` or `shrink` have returned `Err`, the passed pointer
    ///   remains valid.
    ///
    /// [`alloc`]: AllocRef::alloc
    /// [`grow`]: AllocRef::grow
    /// [`shrink`]: AllocRef::shrink
    /// [`dealloc`]: AllocRef::dealloc
    ///
    /// ### Memory fitting
    ///
    /// Some of the methods require that a layout *fit* a memory block. What it means for a layout to
    /// "fit" a memory block means (or equivalently, for a memory block to "fit" a layout) is that the
    /// following conditions must hold:
    ///
    /// * The block must be allocated with the same alignment as [`layout.align()`], and
    ///
    /// * The provided [`layout.size()`] must fall in the range `min ..= max`, where:
    ///   - `min` is the size of the layout most recently used to allocate the block, and
    ///   - `max` is the latest actual size returned from [`alloc`], [`grow`], or [`shrink`].
    ///
    /// [`layout.align()`]: Layout::align
    /// [`layout.size()`]: Layout::size
    ///
    /// # Safety
    ///
    /// * Memory blocks returned from an allocator must point to valid memory and retain their validity
    ///   until the instance and all of its clones are dropped,
    ///
    /// * cloning or moving the allocator must not invalidate memory blocks returned from this
    ///   allocator. A cloned allocator must behave like the same allocator, and
    ///
    /// * any pointer to a memory block which is [*currently allocated*] may be passed to any other
    ///   method of the allocator.
    ///
    /// [*currently allocated*]: #currently-allocated-memory
    //#[unstable(feature = "allocator_api", issue = "32838")]
    pub unsafe trait AllocRef {
        /// Attempts to allocate a block of memory.
        ///
        /// On success, returns a [`MemoryBlock`][] meeting the size and alignment guarantees of `layout`.
        ///
        /// The returned block may have a larger size than specified by `layout.size()` and is
        /// initialized as specified by [`init`], all the way up to the returned size of the block.
        ///
        /// [`init`]: AllocInit
        ///
        /// # Errors
        ///
        /// Returning `Err` indicates that either memory is exhausted or `layout` does not meet
        /// allocator's size or alignment constraints.
        ///
        /// Implementations are encouraged to return `Err` on memory exhaustion rather than panicking or
        /// aborting, but this is not a strict requirement. (Specifically: it is *legal* to implement
        /// this trait atop an underlying native allocation library that aborts on memory exhaustion.)
        ///
        /// Clients wishing to abort computation in response to an allocation error are encouraged to
        /// call the [`handle_alloc_error`] function, rather than directly invoking `panic!` or similar.
        ///
        /// [`handle_alloc_error`]: ../../alloc/alloc/fn.handle_alloc_error.html
        fn alloc(&mut self, layout: Layout, init: AllocInit) -> Result<MemoryBlock, AllocErr>;

        /// Deallocates the memory referenced by `ptr`.
        ///
        /// # Safety
        ///
        /// * `ptr` must denote a block of memory [*currently allocated*] via this allocator, and
        /// * `layout` must [*fit*] that block of memory.
        ///
        /// [*currently allocated*]: #currently-allocated-memory
        /// [*fit*]: #memory-fitting
        unsafe fn dealloc(&mut self, ptr: NonNull<u8>, layout: Layout);

        /// Attempts to extend the memory block.
        ///
        /// Returns a new [`MemoryBlock`][] containing a pointer and the actual size of the allocated
        /// memory. The pointer is suitable for holding data described by a new layout with `layout`’s
        /// alignment and a size given by `new_size`. To accomplish this, the allocator may extend the
        /// allocation referenced by `ptr` to fit the new layout. If the [`placement`] is
        /// [`InPlace`], the returned pointer is guaranteed to be the same as the passed `ptr`.
        ///
        /// If [`MayMove`] is used then ownership of the memory block referenced by `ptr`
        /// is transferred to this allocator. The memory may or may not be freed, and should be
        /// considered unusable (unless of course it is transferred back to the caller again via the
        /// return value of this method).
        ///
        /// If this method returns `Err`, then ownership of the memory block has not been transferred to
        /// this allocator, and the contents of the memory block are unaltered.
        ///
        /// The memory block will contain the following contents after a successful call to `grow`:
        ///   * Bytes `0..layout.size()` are preserved from the original allocation.
        ///   * Bytes `layout.size()..old_size` will either be preserved or initialized according to
        ///     [`init`], depending on the allocator implementation. `old_size` refers to the size of
        ///     the `MemoryBlock` prior to the `grow` call, which may be larger than the size
        ///     that was originally requested when it was allocated.
        ///   * Bytes `old_size..new_size` are initialized according to [`init`]. `new_size` refers to
        ///     the size of the `MemoryBlock` returned by the `grow` call.
        ///
        /// [`InPlace`]: ReallocPlacement::InPlace
        /// [`MayMove`]: ReallocPlacement::MayMove
        /// [`placement`]: ReallocPlacement
        /// [`init`]: AllocInit
        ///
        /// # Safety
        ///
        /// * `ptr` must denote a block of memory [*currently allocated*] via this allocator,
        /// * `layout` must [*fit*] that block of memory (The `new_size` argument need not fit it.),
        // We can't require that `new_size` is strictly greater than `memory.size` because of ZSTs.
        // An alternative would be
        // * `new_size must be strictly greater than `memory.size` or both are zero
        /// * `new_size` must be greater than or equal to `layout.size()`, and
        /// * `new_size`, when rounded up to the nearest multiple of `layout.align()`, must not overflow
        ///   (i.e., the rounded value must be less than or equal to `usize::MAX`).
        ///
        /// [*currently allocated*]: #currently-allocated-memory
        /// [*fit*]: #memory-fitting
        ///
        /// # Errors
        ///
        /// Returns `Err` if the new layout does not meet the allocator's size and alignment
        /// constraints of the allocator, or if growing otherwise fails.
        ///
        /// Implementations are encouraged to return `Err` on memory exhaustion rather than panicking or
        /// aborting, but this is not a strict requirement. (Specifically: it is *legal* to implement
        /// this trait atop an underlying native allocation library that aborts on memory exhaustion.)
        ///
        /// Clients wishing to abort computation in response to an allocation error are encouraged to
        /// call the [`handle_alloc_error`] function, rather than directly invoking `panic!` or similar.
        ///
        /// [`handle_alloc_error`]: ../../alloc/alloc/fn.handle_alloc_error.html
        unsafe fn grow(
            &mut self,
            ptr: NonNull<u8>,
            layout: Layout,
            new_size: usize,
            placement: ReallocPlacement,
            init: AllocInit,
        ) -> Result<MemoryBlock, AllocErr> {
            match placement {
                ReallocPlacement::InPlace => Err(AllocErr),
                ReallocPlacement::MayMove => {
                    let size = layout.size();
                    debug_assert!(
                        new_size >= size,
                        "`new_size` must be greater than or equal to `layout.size()`"
                    );

                    if new_size == size {
                        return Ok(MemoryBlock { ptr, size });
                    }

                    let new_layout =
                        // SAFETY: the caller must ensure that the `new_size` does not overflow.
                        // `layout.align()` comes from a `Layout` and is thus guaranteed to be valid for a Layout.
                        // The caller must ensure that `new_size` is greater than zero.
                        unsafe { Layout::from_size_align_unchecked(new_size, layout.align()) };
                    let new_memory = self.alloc(new_layout, init)?;

                    // SAFETY: because `new_size` must be greater than or equal to `size`, both the old and new
                    // memory allocation are valid for reads and writes for `size` bytes. Also, because the old
                    // allocation wasn't yet deallocated, it cannot overlap `new_memory`. Thus, the call to
                    // `copy_nonoverlapping` is safe.
                    // The safety contract for `dealloc` must be upheld by the caller.
                    unsafe {
                        ptr::copy_nonoverlapping(ptr.as_ptr(), new_memory.ptr.as_ptr(), size);
                        self.dealloc(ptr, layout);
                        Ok(new_memory)
                    }
                }
            }
        }

        /// Attempts to shrink the memory block.
        ///
        /// Returns a new [`MemoryBlock`][] containing a pointer and the actual size of the allocated
        /// memory. The pointer is suitable for holding data described by a new layout with `layout`’s
        /// alignment and a size given by `new_size`. To accomplish this, the allocator may shrink the
        /// allocation referenced by `ptr` to fit the new layout. If the [`placement`] is
        /// [`InPlace`], the returned pointer is guaranteed to be the same as the passed `ptr`.
        ///
        /// If this returns `Ok`, then ownership of the memory block referenced by `ptr` has been
        /// transferred to this allocator. The memory may or may not have been freed, and should be
        /// considered unusable unless it was transferred back to the caller again via the
        /// return value of this method.
        ///
        /// If this method returns `Err`, then ownership of the memory block has not been transferred to
        /// this allocator, and the contents of the memory block are unaltered.
        ///
        /// The behavior of how the allocator tries to shrink the memory is specified by [`placement`].
        ///
        /// [`InPlace`]: ReallocPlacement::InPlace
        /// [`placement`]: ReallocPlacement
        ///
        /// # Safety
        ///
        /// * `ptr` must denote a block of memory [*currently allocated*] via this allocator,
        /// * `layout` must [*fit*] that block of memory (The `new_size` argument need not fit it.), and
        // We can't require that `new_size` is strictly smaller than `memory.size` because of ZSTs.
        // An alternative would be
        // * `new_size must be strictly smaller than `memory.size` or both are zero
        /// * `new_size` must be smaller than or equal to `layout.size()`.
        ///
        /// [*currently allocated*]: #currently-allocated-memory
        /// [*fit*]: #memory-fitting
        ///
        /// # Errors
        ///
        /// Returns `Err` if the new layout does not meet the allocator's size and alignment
        /// constraints of the allocator, or if shrinking otherwise fails.
        ///
        /// Implementations are encouraged to return `Err` on memory exhaustion rather than panicking or
        /// aborting, but this is not a strict requirement. (Specifically: it is *legal* to implement
        /// this trait atop an underlying native allocation library that aborts on memory exhaustion.)
        ///
        /// Clients wishing to abort computation in response to an allocation error are encouraged to
        /// call the [`handle_alloc_error`] function, rather than directly invoking `panic!` or similar.
        ///
        /// [`handle_alloc_error`]: ../../alloc/alloc/fn.handle_alloc_error.html
        unsafe fn shrink(
            &mut self,
            ptr: NonNull<u8>,
            layout: Layout,
            new_size: usize,
            placement: ReallocPlacement,
        ) -> Result<MemoryBlock, AllocErr> {
            match placement {
                ReallocPlacement::InPlace => Err(AllocErr),
                ReallocPlacement::MayMove => {
                    let size = layout.size();
                    debug_assert!(
                        new_size <= size,
                        "`new_size` must be smaller than or equal to `layout.size()`"
                    );

                    if new_size == size {
                        return Ok(MemoryBlock { ptr, size });
                    }

                    let new_layout =
                    // SAFETY: the caller must ensure that the `new_size` does not overflow.
                    // `layout.align()` comes from a `Layout` and is thus guaranteed to be valid for a Layout.
                    // The caller must ensure that `new_size` is greater than zero.
                        unsafe { Layout::from_size_align_unchecked(new_size, layout.align()) };
                    let new_memory = self.alloc(new_layout, AllocInit::Uninitialized)?;

                    // SAFETY: because `new_size` must be lower than or equal to `size`, both the old and new
                    // memory allocation are valid for reads and writes for `new_size` bytes. Also, because the
                    // old allocation wasn't yet deallocated, it cannot overlap `new_memory`. Thus, the call to
                    // `copy_nonoverlapping` is safe.
                    // The safety contract for `dealloc` must be upheld by the caller.
                    unsafe {
                        ptr::copy_nonoverlapping(ptr.as_ptr(), new_memory.ptr.as_ptr(), new_size);
                        self.dealloc(ptr, layout);
                        Ok(new_memory)
                    }
                }
            }
        }

        /// Creates a "by reference" adaptor for this instance of `AllocRef`.
        ///
        /// The returned adaptor also implements `AllocRef` and will simply borrow this.
        #[inline(always)]
        fn by_ref(&mut self) -> &mut Self {
            self
        }
    }

}
