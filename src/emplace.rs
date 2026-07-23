use super::{oom, AllocErr, Bump, RewindGuard};
use core::alloc::Layout;
use core::fmt;
use core::mem::{self, MaybeUninit};
use core::ptr::{self, NonNull};
use core::slice;
use core::str;

/// An in-place allocation builder for a single value of type `T`.
///
/// Created by [`Bump::emplace`] or [`Bump::try_emplace`]. Space for `T` is
/// allocated when the `Emplace` is created, and initialization is deferred
/// until you call [`write`](Emplace::write) or
/// [`assume_init`](Emplace::assume_init).
///
/// If the `Emplace` is dropped without being finalized, the allocation is
/// automatically rewound (reclaimed) in the bump allocator.
///
/// ## Example
///
/// ```
/// let bump = bumpalo::Bump::new();
/// let x = bump.emplace().write(42u64);
/// assert_eq!(*x, 42);
/// ```
pub struct Emplace<'a, T, const MIN_ALIGN: usize> {
    guard: RewindGuard<'a, MIN_ALIGN>,
    ptr: NonNull<MaybeUninit<T>>,
}

impl<T, const MIN_ALIGN: usize> fmt::Debug for Emplace<'_, T, MIN_ALIGN> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Emplace").field("ptr", &self.ptr).finish()
    }
}

impl<'a, T, const MIN_ALIGN: usize> Emplace<'a, T, MIN_ALIGN> {
    /// # Safety
    ///
    /// `guard` must wrap a pointer that is valid for `MaybeUninit<T>`.
    pub(crate) unsafe fn new(guard: RewindGuard<'a, MIN_ALIGN>) -> Self {
        let ptr = guard.ptr.cast::<MaybeUninit<T>>();
        Self { guard, ptr }
    }

    /// Get a shared reference to the underlying `MaybeUninit<T>`.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let place = bump.emplace::<u64>();
    /// let uninit: &std::mem::MaybeUninit<u64> = place.as_uninit();
    /// drop(place);
    /// ```
    #[inline]
    pub fn as_uninit(&self) -> &MaybeUninit<T> {
        unsafe { self.ptr.as_ref() }
    }

    /// Get an exclusive reference to the underlying `MaybeUninit<T>`.
    ///
    /// ## Example
    ///
    /// ```
    /// use std::mem::MaybeUninit;
    ///
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace::<u32>();
    /// place.as_uninit_mut().write(99);
    /// let val = unsafe { place.assume_init() };
    /// assert_eq!(*val, 99);
    /// ```
    #[inline]
    pub fn as_uninit_mut(&mut self) -> &mut MaybeUninit<T> {
        unsafe { self.ptr.as_mut() }
    }

    /// Finalize this place, assuming the value has been initialized.
    ///
    /// # Safety
    ///
    /// The caller must have fully initialized the `MaybeUninit<T>` (e.g. via
    /// [`as_uninit_mut`](Emplace::as_uninit_mut)) before calling this method.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace::<u32>();
    /// place.as_uninit_mut().write(42);
    /// let val = unsafe { place.assume_init() };
    /// assert_eq!(*val, 42);
    /// ```
    #[inline]
    pub unsafe fn assume_init(self) -> &'a mut T {
        self.guard.finish();
        unsafe { &mut *self.ptr.as_ptr().cast::<T>() }
    }

    /// Finalize this place, returning the raw `MaybeUninit<T>` without
    /// requiring initialization.
    ///
    /// The caller is responsible for ensuring the memory is properly
    /// initialized before the returned reference is read.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let place = bump.emplace::<u64>();
    /// let uninit: &mut std::mem::MaybeUninit<u64> = place.into_uninit();
    /// uninit.write(123);
    /// let val = unsafe { uninit.assume_init_ref() };
    /// assert_eq!(*val, 123);
    /// ```
    #[inline]
    pub fn into_uninit(self) -> &'a mut MaybeUninit<T> {
        self.guard.finish();
        unsafe { &mut *self.ptr.as_ptr() }
    }

    /// Write a value into this place and return an exclusive reference to it.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let x = bump.emplace().write(42u64);
    /// assert_eq!(*x, 42);
    /// ```
    #[inline]
    pub fn write(self, value: T) -> &'a mut T {
        unsafe {
            ptr::write(self.ptr.as_ptr().cast::<T>(), value);
            self.assume_init()
        }
    }
}

/// An in-place allocation builder for slices of `T`.
///
/// Created by [`Bump::emplace_slice`], [`Bump::emplace_slice_with_capacity`],
/// or their `try_*` variants. Elements can be added incrementally via
/// [`push`](EmplaceSlice::push), [`extend`](EmplaceSlice::extend), or
/// [`extend_from_slice_copy`](EmplaceSlice::extend_from_slice_copy), or all
/// at once via [`copy_slice`](EmplaceSlice::copy_slice),
/// [`clone_slice`](EmplaceSlice::clone_slice), or
/// [`write_iter`](EmplaceSlice::write_iter).
///
/// Finalize with [`into_mut_slice`](EmplaceSlice::into_mut_slice) or
/// [`into_slice`](EmplaceSlice::into_slice) to get the resulting bump-allocated
/// slice. If dropped without finalizing, the allocation is automatically
/// rewound.
///
/// ## Example
///
/// ```
/// let bump = bumpalo::Bump::new();
/// let mut place = bump.emplace_slice::<u8>();
/// place.push(1);
/// place.push(2);
/// place.push(3);
/// let slice = place.into_mut_slice();
/// assert_eq!(slice, &[1, 2, 3]);
/// ```
pub struct EmplaceSlice<'a, T, const MIN_ALIGN: usize> {
    inner: Emplace<'a, T, MIN_ALIGN>,
    len: usize,
    capacity: usize,
}

impl<T, const MIN_ALIGN: usize> fmt::Debug for EmplaceSlice<'_, T, MIN_ALIGN> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EmplaceSlice")
            .field("ptr", &self.base_ptr())
            .field("len", &self.len)
            .field("capacity", &self.capacity)
            .finish()
    }
}

impl<'a, T, const MIN_ALIGN: usize> EmplaceSlice<'a, T, MIN_ALIGN>
where
    T: Sized,
{
    /// # Safety
    ///
    /// `guard` must wrap a pointer that is valid for `MaybeUninit<T>`.
    pub(crate) unsafe fn new(guard: RewindGuard<'a, MIN_ALIGN>, capacity: usize) -> Self {
        Self {
            inner: Emplace::new(guard),
            len: 0,
            capacity,
        }
    }

    fn base_ptr(&self) -> *mut T {
        self.inner.ptr.as_ptr().cast::<T>()
    }

    fn bump(&self) -> &'a Bump<MIN_ALIGN> {
        self.inner.guard.bump
    }

    /// Get the full capacity as a shared slice of `MaybeUninit<T>`.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let place = bump.emplace_slice_with_capacity::<u8>(4);
    /// assert!(place.as_uninit().len() >= 4);
    /// drop(place);
    /// ```
    #[inline]
    pub fn as_uninit(&self) -> &[MaybeUninit<T>] {
        unsafe { slice::from_raw_parts(self.base_ptr().cast::<MaybeUninit<T>>(), self.capacity) }
    }

    /// Get the full capacity as an exclusive slice of `MaybeUninit<T>`.
    ///
    /// ## Example
    ///
    /// ```
    /// use std::mem::MaybeUninit;
    ///
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_slice_with_capacity::<u32>(4);
    /// place.as_uninit_mut()[0] = MaybeUninit::new(42);
    /// drop(place);
    /// ```
    #[inline]
    pub fn as_uninit_mut(&mut self) -> &mut [MaybeUninit<T>] {
        unsafe {
            slice::from_raw_parts_mut(self.base_ptr().cast::<MaybeUninit<T>>(), self.capacity)
        }
    }

    /// Finalize this place, assuming all `self.len()` elements are initialized.
    ///
    /// Shrinks the allocation to fit, then returns the initialized slice.
    ///
    /// # Safety
    ///
    /// All `self.len()` elements must have been written (e.g. via
    /// [`push`](EmplaceSlice::push) or manual writes to
    /// [`as_uninit_mut`](EmplaceSlice::as_uninit_mut)).
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_slice::<u8>();
    /// place.push(1);
    /// place.push(2);
    /// let s = unsafe { place.assume_init() };
    /// assert_eq!(s, &[1, 2]);
    /// ```
    #[inline]
    pub unsafe fn assume_init(mut self) -> &'a mut [T] {
        self.shrink(self.len);
        let len = self.len;
        let ptr = self.base_ptr();
        self.inner.guard.finish();
        unsafe { slice::from_raw_parts_mut(ptr, len) }
    }

    /// Finalize this place, returning the full capacity as an exclusive slice
    /// of `MaybeUninit<T>` without requiring initialization.
    ///
    /// The caller is responsible for ensuring that any positions they read from
    /// the returned slice have been properly initialized.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let place = bump.emplace_slice_with_capacity::<u32>(4);
    /// let uninit = place.into_uninit();
    /// assert!(uninit.len() >= 4);
    /// ```
    #[inline]
    pub fn into_uninit(self) -> &'a mut [MaybeUninit<T>] {
        let cap = self.capacity;
        let ptr = self.base_ptr().cast::<MaybeUninit<T>>();
        self.inner.guard.finish();
        unsafe { slice::from_raw_parts_mut(ptr, cap) }
    }

    /// Get the element capacity of this `[T]` place.
    ///
    /// This is the number of elements that can be held without reallocating.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let place = bump.emplace_slice_with_capacity::<u8>(4);
    /// assert!(place.capacity() >= 4);
    /// ```
    #[inline]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get the number of initialized elements in this place.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_slice::<u8>();
    /// assert_eq!(place.len(), 0);
    /// place.push(1);
    /// assert_eq!(place.len(), 1);
    /// ```
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if no elements have been written.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_slice::<u8>();
    /// assert!(place.is_empty());
    /// place.push(1);
    /// assert!(!place.is_empty());
    /// ```
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the initialized elements as a shared slice.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_slice::<u8>();
    /// place.push(1);
    /// place.push(2);
    /// assert_eq!(place.as_slice(), &[1, 2]);
    /// ```
    #[inline]
    pub fn as_slice(&self) -> &[T] {
        unsafe { slice::from_raw_parts(self.base_ptr(), self.len) }
    }

    /// Get the initialized elements as an exclusive slice.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_slice::<u8>();
    /// place.push(10);
    /// place.push(20);
    /// place.as_mut_slice()[0] = 99;
    /// assert_eq!(place.as_slice(), &[99, 20]);
    /// ```
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        unsafe { slice::from_raw_parts_mut(self.base_ptr(), self.len) }
    }

    /// Finalize this place and return the initialized elements as a shared
    /// slice.
    ///
    /// The allocation is shrunk to fit the initialized elements.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_slice::<u32>();
    /// place.push(10);
    /// place.push(20);
    /// let s: &[u32] = place.into_slice();
    /// assert_eq!(s, &[10, 20]);
    /// ```
    #[inline]
    pub fn into_slice(self) -> &'a [T] {
        self.into_mut_slice()
    }

    /// Finalize this place and return the initialized elements as an exclusive
    /// slice.
    ///
    /// The allocation is shrunk to fit the initialized elements.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_slice::<u32>();
    /// place.push(10);
    /// place.push(20);
    /// let s = place.into_mut_slice();
    /// s[0] = 99;
    /// assert_eq!(s, &[99, 20]);
    /// ```
    #[inline]
    pub fn into_mut_slice(mut self) -> &'a mut [T] {
        self.shrink(self.len);
        let len = self.len;
        let ptr = self.base_ptr();
        self.inner.guard.finish();
        unsafe { slice::from_raw_parts_mut(ptr, len) }
    }

    /// Shrink this place to the given new capacity.
    ///
    /// This is a no-op if `new_capacity >= self.capacity()`. The capacity will
    /// never be reduced below `self.len()`.
    ///
    /// If shrinking fails (e.g. because this is not the last allocation), the
    /// allocation is left as-is.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_slice_with_capacity::<u8>(16);
    /// place.push(1);
    /// place.push(2);
    /// place.shrink(4);
    /// assert!(place.capacity() <= 4);
    /// ```
    #[inline]
    pub fn shrink(&mut self, new_capacity: usize) {
        let _ = self.try_shrink(new_capacity);
    }

    /// Try to shrink this place to the given new capacity.
    ///
    /// This is a no-op if `new_capacity >= self.capacity()`. The capacity will
    /// never be reduced below `self.len()`.
    ///
    /// ## Errors
    ///
    /// Returns an error if the underlying allocation cannot be shrunk.
    ///
    /// ## Example
    ///
    /// ```
    /// # fn foo() -> Result<(), bumpalo::AllocErr> {
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_slice_with_capacity::<u8>(16);
    /// place.push(1);
    /// place.try_shrink(4)?;
    /// assert!(place.capacity() <= 4);
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    #[inline]
    pub fn try_shrink(&mut self, new_capacity: usize) -> Result<(), AllocErr> {
        if new_capacity >= self.capacity || mem::size_of::<T>() == 0 {
            return Ok(());
        }
        debug_assert!(new_capacity >= self.len);
        let new_capacity = new_capacity.max(self.len);

        let old_layout = Layout::array::<T>(self.capacity).unwrap();
        let new_layout = Layout::array::<T>(new_capacity).unwrap();

        let ptr = self.inner.guard.ptr;
        let new_ptr = unsafe { Bump::shrink(self.bump(), ptr, old_layout, new_layout)? };
        self.inner.guard.ptr = new_ptr;
        self.inner.ptr = new_ptr.cast();
        self.capacity = new_capacity;
        Ok(())
    }

    /// Shrink this place's capacity to exactly its length.
    ///
    /// If shrinking fails (e.g. because this is not the last allocation), the
    /// allocation is left as-is.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_slice_with_capacity::<u8>(16);
    /// place.push(1);
    /// place.push(2);
    /// place.shrink_to_fit();
    /// assert!(place.capacity() <= 2);
    /// ```
    #[inline]
    pub fn shrink_to_fit(&mut self) {
        self.shrink(self.len);
    }

    /// Try to shrink this place's capacity to exactly its length.
    ///
    /// ## Errors
    ///
    /// Returns an error if the underlying allocation cannot be shrunk.
    ///
    /// ## Example
    ///
    /// ```
    /// # fn foo() -> Result<(), bumpalo::AllocErr> {
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_slice_with_capacity::<u8>(16);
    /// place.push(1);
    /// place.try_shrink_to_fit()?;
    /// assert!(place.capacity() <= 1);
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    #[inline]
    pub fn try_shrink_to_fit(&mut self) -> Result<(), AllocErr> {
        self.try_shrink(self.len)
    }

    fn try_grow(&mut self, new_capacity: usize) -> Result<(), AllocErr> {
        if new_capacity <= self.capacity {
            return Ok(());
        }

        if mem::size_of::<T>() == 0 {
            self.capacity = new_capacity;
            return Ok(());
        }

        let old_layout = Layout::array::<T>(self.capacity).map_err(|_| AllocErr)?;
        let new_layout = Layout::array::<T>(new_capacity).map_err(|_| AllocErr)?;

        let ptr = self.inner.guard.ptr;
        let new_ptr = unsafe { Bump::grow_internal(self.bump(), ptr, old_layout, new_layout)? };

        self.inner.guard.ptr = new_ptr;
        self.inner.ptr = new_ptr.cast();
        self.capacity = new_capacity;
        Ok(())
    }

    /// Reserve capacity for at least `additional` more elements.
    ///
    /// ## Panics
    ///
    /// Panics if the allocation cannot be grown.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_slice::<u8>();
    /// place.reserve(10);
    /// assert!(place.capacity() >= 10);
    /// ```
    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        self.try_reserve(additional).unwrap_or_else(|_| oom())
    }

    /// Try to reserve capacity for at least `additional` more elements.
    ///
    /// Attempts to double capacity first; if that fails, falls back to
    /// reserving exactly the amount needed.
    ///
    /// ## Errors
    ///
    /// Returns an error if the allocation cannot be grown.
    ///
    /// ## Example
    ///
    /// ```
    /// # fn foo() -> Result<(), bumpalo::AllocErr> {
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_slice::<u8>();
    /// place.try_reserve(10)?;
    /// assert!(place.capacity() >= 10);
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    #[inline]
    pub fn try_reserve(&mut self, additional: usize) -> Result<(), AllocErr> {
        let needed = self.len.checked_add(additional).ok_or(AllocErr)?;
        if needed <= self.capacity {
            return Ok(());
        }

        // Try doubling first.
        let doubled = self.capacity.saturating_mul(2).max(needed);
        if self.try_grow(doubled).is_ok() {
            return Ok(());
        }

        // Fall back to exactly what's needed.
        self.try_grow(needed)
    }

    /// Reserve capacity for exactly `additional` more elements.
    ///
    /// ## Panics
    ///
    /// Panics if the allocation cannot be grown.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_slice::<u8>();
    /// place.reserve_exact(5);
    /// assert!(place.capacity() >= 5);
    /// ```
    #[inline]
    pub fn reserve_exact(&mut self, additional: usize) {
        self.try_reserve_exact(additional).unwrap_or_else(|_| oom())
    }

    /// Try to reserve capacity for exactly `additional` more elements.
    ///
    /// ## Errors
    ///
    /// Returns an error if the allocation cannot be grown.
    ///
    /// ## Example
    ///
    /// ```
    /// # fn foo() -> Result<(), bumpalo::AllocErr> {
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_slice::<u8>();
    /// place.try_reserve_exact(5)?;
    /// assert!(place.capacity() >= 5);
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    #[inline]
    pub fn try_reserve_exact(&mut self, additional: usize) -> Result<(), AllocErr> {
        let needed = self.len.checked_add(additional).ok_or(AllocErr)?;
        self.try_grow(needed)
    }

    /// Push an element onto this place, growing if necessary.
    ///
    /// ## Panics
    ///
    /// Panics if the place is at capacity and growing the allocation fails.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_slice::<u8>();
    /// place.push(1);
    /// place.push(2);
    /// assert_eq!(place.as_slice(), &[1, 2]);
    /// ```
    #[inline]
    pub fn push(&mut self, element: T) {
        self.try_push(element).unwrap_or_else(|_| oom())
    }

    /// Try to push an element, growing if necessary.
    ///
    /// Returns `Err(element)` if the place is at capacity and growing fails,
    /// giving back ownership of the element.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_slice::<u8>();
    /// assert!(place.try_push(42).is_ok());
    /// assert_eq!(place.as_slice(), &[42]);
    /// ```
    #[inline]
    pub fn try_push(&mut self, element: T) -> Result<(), T> {
        debug_assert!(self.len <= self.capacity);
        if self.len == self.capacity {
            if self.try_reserve(1).is_err() {
                return Err(element);
            }
        }
        unsafe {
            ptr::write(self.base_ptr().add(self.len), element);
        }
        self.len += 1;
        Ok(())
    }

    /// Extend this place with elements from an iterator, growing as needed.
    ///
    /// ## Panics
    ///
    /// Panics if growing the allocation fails.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_slice::<u8>();
    /// place.extend(0..5);
    /// assert_eq!(place.as_slice(), &[0, 1, 2, 3, 4]);
    /// ```
    #[inline]
    pub fn extend(&mut self, iter: impl IntoIterator<Item = T>) {
        for element in iter {
            self.push(element);
        }
    }

    /// Try to extend this place with elements from an iterator, growing as
    /// needed.
    ///
    /// ## Errors
    ///
    /// Returns an error if growing the allocation fails. Elements already
    /// pushed before the failure remain in the place.
    ///
    /// ## Example
    ///
    /// ```
    /// # fn foo() -> Result<(), bumpalo::AllocErr> {
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_slice::<u8>();
    /// place.try_extend(0..3)?;
    /// assert_eq!(place.as_slice(), &[0, 1, 2]);
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    #[inline]
    pub fn try_extend(&mut self, iter: impl IntoIterator<Item = T>) -> Result<(), AllocErr> {
        let iter = iter.into_iter();
        let (min, max) = iter.size_hint();
        self.try_reserve(max.unwrap_or(min))?;
        for element in iter {
            if self.try_push(element).is_err() {
                return Err(AllocErr);
            }
        }
        Ok(())
    }

    /// Extend this place by copying elements from a slice.
    ///
    /// ## Panics
    ///
    /// Panics if growing the allocation fails.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_slice::<u8>();
    /// place.extend_from_slice_copy(&[1, 2, 3]);
    /// assert_eq!(place.as_slice(), &[1, 2, 3]);
    /// ```
    #[inline]
    pub fn extend_from_slice_copy(&mut self, src: &[T])
    where
        T: Copy,
    {
        self.try_extend_from_slice_copy(src)
            .unwrap_or_else(|_| oom())
    }

    /// Try to extend this place by copying elements from a slice.
    ///
    /// ## Errors
    ///
    /// Returns an error if growing the allocation fails.
    ///
    /// ## Example
    ///
    /// ```
    /// # fn foo() -> Result<(), bumpalo::AllocErr> {
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_slice::<u8>();
    /// place.try_extend_from_slice_copy(&[1, 2, 3])?;
    /// assert_eq!(place.as_slice(), &[1, 2, 3]);
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    #[inline]
    pub fn try_extend_from_slice_copy(&mut self, src: &[T]) -> Result<(), AllocErr>
    where
        T: Copy,
    {
        self.try_reserve_exact(src.len())?;
        unsafe {
            ptr::copy_nonoverlapping(src.as_ptr(), self.base_ptr().add(self.len), src.len());
        }
        self.len += src.len();
        Ok(())
    }

    /// Extend this place by cloning elements from a slice.
    ///
    /// ## Panics
    ///
    /// Panics if growing the allocation fails.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_slice::<u8>();
    /// place.extend_from_slice_clone(&[1, 2, 3]);
    /// assert_eq!(place.as_slice(), &[1, 2, 3]);
    /// ```
    #[inline]
    pub fn extend_from_slice_clone(&mut self, src: &[T])
    where
        T: Clone,
    {
        self.try_extend_from_slice_clone(src)
            .unwrap_or_else(|_| oom())
    }

    /// Try to extend this place by cloning elements from a slice.
    ///
    /// ## Errors
    ///
    /// Returns an error if growing the allocation fails.
    ///
    /// ## Example
    ///
    /// ```
    /// # fn foo() -> Result<(), bumpalo::AllocErr> {
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_slice::<u8>();
    /// place.try_extend_from_slice_clone(&[1, 2, 3])?;
    /// assert_eq!(place.as_slice(), &[1, 2, 3]);
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    #[inline]
    pub fn try_extend_from_slice_clone(&mut self, src: &[T]) -> Result<(), AllocErr>
    where
        T: Clone,
    {
        self.try_reserve_exact(src.len())?;
        for val in src.iter().cloned() {
            unsafe {
                ptr::write(self.base_ptr().add(self.len), val);
            }
            self.len += 1;
        }
        Ok(())
    }

    /// Consume an iterator, push all elements, shrink to fit, and finalize.
    ///
    /// ## Panics
    ///
    /// Panics if growing the allocation fails.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let place = bump.emplace_slice::<u8>();
    /// let s = place.write_iter(0..5);
    /// assert_eq!(s, &[0, 1, 2, 3, 4]);
    /// ```
    #[inline]
    pub fn write_iter(mut self, iter: impl IntoIterator<Item = T>) -> &'a mut [T] {
        self.extend(iter);
        self.into_mut_slice()
    }

    /// Try to consume an iterator, push all elements, shrink to fit, and
    /// finalize.
    ///
    /// ## Errors
    ///
    /// Returns an error if growing the allocation fails.
    ///
    /// ## Example
    ///
    /// ```
    /// # fn foo() -> Result<(), bumpalo::AllocErr> {
    /// let bump = bumpalo::Bump::new();
    /// let place = bump.emplace_slice::<u8>();
    /// let s = place.try_write_iter(0..5)?;
    /// assert_eq!(s, &[0, 1, 2, 3, 4]);
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    #[inline]
    pub fn try_write_iter(
        mut self,
        iter: impl IntoIterator<Item = T>,
    ) -> Result<&'a mut [T], AllocErr> {
        self.try_extend(iter)?;
        Ok(self.into_mut_slice())
    }

    /// Copy a slice into this place, growing if necessary, and finalize.
    ///
    /// ## Panics
    ///
    /// Panics if growing the allocation fails.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let data = [1u8, 2, 3, 4];
    /// let s = bump.emplace_slice_with_capacity(data.len()).copy_slice(&data);
    /// assert_eq!(s, &[1, 2, 3, 4]);
    /// ```
    #[inline]
    pub fn copy_slice(self, from: &[T]) -> &'a mut [T]
    where
        T: Copy,
    {
        self.try_copy_slice(from).unwrap_or_else(|_| oom())
    }

    /// Try to copy a slice into this place, growing if necessary, and
    /// finalize.
    ///
    /// ## Errors
    ///
    /// Returns an error if growing the allocation fails.
    ///
    /// ## Example
    ///
    /// ```
    /// # fn foo() -> Result<(), bumpalo::AllocErr> {
    /// let bump = bumpalo::Bump::new();
    /// let data = [1u8, 2, 3];
    /// let s = bump.emplace_slice_with_capacity(data.len()).try_copy_slice(&data)?;
    /// assert_eq!(s, &[1, 2, 3]);
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    #[inline]
    pub fn try_copy_slice(mut self, from: &[T]) -> Result<&'a mut [T], AllocErr>
    where
        T: Copy,
    {
        self.try_reserve_exact(from.len().saturating_sub(self.capacity))?;
        unsafe {
            ptr::copy_nonoverlapping(from.as_ptr(), self.base_ptr(), from.len());
        }
        self.len = from.len();
        Ok(self.into_mut_slice())
    }

    /// Clone a slice into this place, growing if necessary, and finalize.
    ///
    /// ## Panics
    ///
    /// Panics if growing the allocation fails.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let data = [1u8, 2, 3];
    /// let s = bump.emplace_slice_with_capacity(data.len()).clone_slice(&data);
    /// assert_eq!(s, &[1, 2, 3]);
    /// ```
    #[inline]
    pub fn clone_slice(self, from: &[T]) -> &'a mut [T]
    where
        T: Clone,
    {
        self.try_clone_slice(from).unwrap_or_else(|_| oom())
    }

    /// Try to clone a slice into this place, growing if necessary, and
    /// finalize.
    ///
    /// ## Errors
    ///
    /// Returns an error if growing the allocation fails.
    ///
    /// ## Example
    ///
    /// ```
    /// # fn foo() -> Result<(), bumpalo::AllocErr> {
    /// let bump = bumpalo::Bump::new();
    /// let data = [1u8, 2, 3];
    /// let s = bump.emplace_slice_with_capacity(data.len()).try_clone_slice(&data)?;
    /// assert_eq!(s, &[1, 2, 3]);
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    #[inline]
    pub fn try_clone_slice(mut self, from: &[T]) -> Result<&'a mut [T], AllocErr>
    where
        T: Clone,
    {
        self.try_reserve_exact(from.len().saturating_sub(self.capacity))?;
        for (i, val) in from.iter().cloned().enumerate() {
            unsafe {
                ptr::write(self.base_ptr().add(i), val);
            }
        }
        self.len = from.len();
        Ok(self.into_mut_slice())
    }
}

/// An in-place allocation builder for UTF-8 string slices.
///
/// Created by [`Bump::emplace_str`], [`Bump::emplace_str_with_capacity`], or
/// their `try_*` variants.
///
/// ## Example
///
/// ```
/// let bump = bumpalo::Bump::new();
/// let mut place = bump.emplace_str();
/// place.push_str("hello, ");
/// place.push_str("world!");
/// let s = place.into_str();
/// assert_eq!(s, "hello, world!");
/// ```
#[derive(Debug)]
pub struct EmplaceStr<'a, const MIN_ALIGN: usize> {
    inner: EmplaceSlice<'a, u8, MIN_ALIGN>,
}

impl<'a, const MIN_ALIGN: usize> EmplaceStr<'a, MIN_ALIGN> {
    pub(crate) unsafe fn new(inner: EmplaceSlice<'a, u8, MIN_ALIGN>) -> Self {
        Self { inner }
    }

    /// Get the number of bytes written into this place thus far.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_str();
    /// assert_eq!(place.len(), 0);
    /// place.push_str("hi");
    /// assert_eq!(place.len(), 2);
    /// ```
    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` if no bytes have been written.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_str();
    /// assert!(place.is_empty());
    /// place.push('a');
    /// assert!(!place.is_empty());
    /// ```
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Get the byte capacity of this string place.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let place = bump.emplace_str_with_capacity(16);
    /// assert!(place.capacity() >= 16);
    /// ```
    #[inline]
    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    /// View the initialized portion as a string slice.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_str();
    /// place.push_str("hello");
    /// assert_eq!(place.as_str(), "hello");
    /// ```
    #[inline]
    pub fn as_str(&self) -> &str {
        unsafe { str::from_utf8_unchecked(self.inner.as_slice()) }
    }

    /// View the initialized portion as a mutable string slice.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_str();
    /// place.push_str("hello");
    /// place.as_str_mut().make_ascii_uppercase();
    /// assert_eq!(place.as_str(), "HELLO");
    /// ```
    #[inline]
    pub fn as_str_mut(&mut self) -> &mut str {
        unsafe { str::from_utf8_unchecked_mut(self.inner.as_mut_slice()) }
    }

    /// Append a string slice to this place.
    ///
    /// ## Panics
    ///
    /// Panics if growing the allocation fails.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_str();
    /// place.push_str("hello");
    /// place.push_str(" world");
    /// assert_eq!(place.as_str(), "hello world");
    /// ```
    #[inline]
    pub fn push_str(&mut self, s: &str) {
        self.try_push_str(s).unwrap_or_else(|_| oom())
    }

    /// Try to append a string slice to this place.
    ///
    /// ## Errors
    ///
    /// Returns an error if growing the allocation fails.
    ///
    /// ## Example
    ///
    /// ```
    /// # fn foo() -> Result<(), bumpalo::AllocErr> {
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_str();
    /// place.try_push_str("hello")?;
    /// assert_eq!(place.as_str(), "hello");
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    #[inline]
    pub fn try_push_str(&mut self, s: &str) -> Result<(), AllocErr> {
        self.inner.try_extend_from_slice_copy(s.as_bytes())
    }

    /// Append a character to this place (encoded as UTF-8).
    ///
    /// ## Panics
    ///
    /// Panics if growing the allocation fails.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_str();
    /// place.push('h');
    /// place.push('i');
    /// assert_eq!(place.as_str(), "hi");
    /// ```
    #[inline]
    pub fn push(&mut self, c: char) {
        self.try_push(c).unwrap_or_else(|_| oom())
    }

    /// Try to append a character to this place (encoded as UTF-8).
    ///
    /// ## Errors
    ///
    /// Returns an error if growing the allocation fails.
    ///
    /// ## Example
    ///
    /// ```
    /// # fn foo() -> Result<(), bumpalo::AllocErr> {
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_str();
    /// place.try_push('h')?;
    /// place.try_push('i')?;
    /// assert_eq!(place.as_str(), "hi");
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    #[inline]
    pub fn try_push(&mut self, c: char) -> Result<(), AllocErr> {
        let mut buf = [0u8; 4];
        let s = c.encode_utf8(&mut buf);
        self.try_push_str(s)
    }

    /// Write a string into this place and finalize.
    ///
    /// ## Panics
    ///
    /// Panics if growing the allocation fails.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let s = bump.emplace_str_with_capacity(5).write_str("hello");
    /// assert_eq!(s, "hello");
    /// ```
    #[inline]
    pub fn write_str(mut self, s: &str) -> &'a mut str {
        self.push_str(s);
        self.into_str_mut()
    }

    /// Try to write a string into this place and finalize.
    ///
    /// ## Errors
    ///
    /// Returns an error if growing the allocation fails.
    ///
    /// ## Example
    ///
    /// ```
    /// # fn foo() -> Result<(), bumpalo::AllocErr> {
    /// let bump = bumpalo::Bump::new();
    /// let s = bump.emplace_str_with_capacity(5).try_write_str("hello")?;
    /// assert_eq!(s, "hello");
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    #[inline]
    pub fn try_write_str(mut self, s: &str) -> Result<&'a mut str, AllocErr> {
        self.try_push_str(s)?;
        Ok(self.into_str_mut())
    }

    /// Finalize this place, returning the initialized portion as `&str`.
    ///
    /// The allocation is shrunk to fit.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_str();
    /// place.push_str("hello");
    /// let s: &str = place.into_str();
    /// assert_eq!(s, "hello");
    /// ```
    #[inline]
    pub fn into_str(self) -> &'a str {
        self.into_str_mut()
    }

    /// Finalize this place, returning the initialized portion as `&mut str`.
    ///
    /// The allocation is shrunk to fit.
    ///
    /// ## Example
    ///
    /// ```
    /// let bump = bumpalo::Bump::new();
    /// let mut place = bump.emplace_str();
    /// place.push_str("hello");
    /// let s: &mut str = place.into_str_mut();
    /// s.make_ascii_uppercase();
    /// assert_eq!(s, "HELLO");
    /// ```
    #[inline]
    pub fn into_str_mut(self) -> &'a mut str {
        let slice = self.inner.into_mut_slice();
        unsafe { str::from_utf8_unchecked_mut(slice) }
    }
}
