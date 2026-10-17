//! This module provides integration with the `zerocopy` crate. `zerocopy` defines traits and
//! methods which allow for _safely_ transmuting types and for safely converting types to/from
//! byte buffers.

use crate::Bump;
use core::ptr::NonNull;
use core_alloc::alloc::Layout;
use zerocopy::FromZeroes;

impl Bump {
    /// Allocates `T` by filling it with zeroes.
    ///
    /// This function allocates `T` directly in the `Bump` without initializing an instance of it
    /// on the stack. This is possible due to the `T: FromZeroes` constraint, which specifies that
    /// `T` may be safely constructed from an all-zeroes bit pattern.
    ///
    /// This avoids the "placement new" problem with using `Box::new()` on large types. Allocating
    /// a large type using `Box::new()` requires first constructing the large type on the stack,
    /// then moving it into a heap allocation. This can cause stack overflow.  Using
    /// `Bump::alloc_zeroed` avoids this problem.
    ///
    /// # Example
    ///
    /// ```
    /// # use bumpalo::Bump;
    ///
    /// #[repr(C)]
    /// #[derive(zerocopy_derive::FromZeroes)]
    /// struct MyData {
    ///     x: u32,
    ///     y: u8,
    ///     big_buffer: [u8; 0x10000],
    /// }
    ///
    /// let b = Bump::new();
    /// let my_data: &mut MyData = b.alloc_zeroed();
    /// my_data.big_buffer[0] = 42;
    /// ```
    pub fn alloc_zeroed<T: FromZeroes>(&self) -> &mut T {
        let layout = Layout::new::<T>();
        if layout.size() == 0 {
            // SAFETY: For ZSTs, NonNull::dangling() is a permissible address.
            unsafe {
                return NonNull::dangling().as_mut();
            }
        }

        let p = self.alloc_layout(layout);

        // SAFETY: The FromZeroes trait means means that zero-filling this allocation is a valid
        // initialization of it, for T.
        unsafe {
            p.as_ptr().write_bytes(0, layout.size());
            &mut *(p.as_ptr() as *mut T)
        }
    }

    /// Allocates `[T]` of the given length by filling it with zeroes.
    ///
    /// # Example
    ///
    /// ```
    /// # use bumpalo::Bump;
    /// #[repr(C)]
    /// #[derive(zerocopy_derive::FromZeroes)]
    /// struct MyData {
    ///     x: u32,
    ///     y: u8,
    ///     big_buffer: [u8; 0x10000],
    /// }
    ///
    /// let b = Bump::new();
    /// let my_data: &mut [MyData] = b.alloc_slice_zeroed(1000);
    /// my_data[0].big_buffer[0] = 42;
    /// ```
    pub fn alloc_slice_zeroed<T: FromZeroes>(&self, len: usize) -> &mut [T] {
        if len == 0 {
            return &mut [];
        }

        if core::mem::size_of::<T>() == 0 {
            // SAFETY: For ZSTs, NonNull::dangling() is a permissible address, even for arrays.
            unsafe {
                return core::slice::from_raw_parts_mut(NonNull::dangling().as_mut(), len);
            }
        }

        let layout = Layout::array::<T>(len).unwrap();
        let p = self.alloc_layout(layout);

        // SAFETY: The FromZeroes trait means means that zero-filling this allocation is a valid
        // initialization of it, for T.
        unsafe {
            p.as_ptr().write_bytes(0, layout.size());
            core::slice::from_raw_parts_mut(p.as_ptr() as *mut T, len)
        }
    }
}
