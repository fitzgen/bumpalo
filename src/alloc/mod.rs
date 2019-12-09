//! Memory allocation APIs

pub use core::alloc::{Layout, LayoutErr};

#[cfg(feature = "unstable_core_alloc")]
pub use core::alloc::{Alloc, Excess, AllocErr, CannotReallocInPlace};
#[cfg(feature = "unstable_core_alloc")]
pub type UnstableLayoutMethods = ();

#[cfg(not(feature = "unstable_core_alloc"))]
mod stable;
#[cfg(not(feature = "unstable_core_alloc"))]
pub use stable::{Alloc, Excess, AllocErr, CannotReallocInPlace, UnstableLayoutMethods};

pub fn handle_alloc_error(layout: Layout) -> ! {
    panic!("encountered allocation error: {:?}", layout)
}

